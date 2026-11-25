//! Lokale MCP-server (JSON-RPC over HTTP) in Studio. Claude Code koppelt via `--mcp-config` en kan
//! zo het project sturen. De server luistert alleen op 127.0.0.1, eist een geheim bearer-token dat
//! Studio bij elke start nieuw maakt, en controleert de Host-header (tegen DNS-rebinding).

use crate::db::Db;
use crate::permissions::{Decision, Permissions};
use crate::studio_tools as tools;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

const MAX_BODY: usize = 1024 * 1024;

#[derive(Clone)]
pub struct McpInfo {
    pub port: u16,
    pub token: String,
}

pub struct Deps {
    pub db: Arc<Db>,
    pub perms: Arc<Permissions>,
}

pub fn tool_list() -> Value {
    let no_args = json!({"type":"object","properties":{}});
    json!([
        {"name":"project_status","description":"Name, version, status, git state and checklist progress of this app.","inputSchema":no_args},
        {"name":"list_screens","description":"List the screens (widget classes ending in Screen or Page) in lib/.","inputSchema":no_args},
        {"name":"get_theme","description":"Current Studio-managed theme (seed color).","inputSchema":no_args},
        {"name":"set_theme","description":"Set the app's seed color and regenerate lib/app_theme.dart.",
         "inputSchema":{"type":"object","properties":{"seedColor":{"type":"string","description":"Hex color like #4F46E5"}},"required":["seedColor"]}},
        {"name":"preview_status","description":"Whether the live preview is running.","inputSchema":no_args},
        {"name":"preview_screenshot","description":"Screenshot of the live preview (not available yet).","inputSchema":no_args},
        {"name":"create_snapshot","description":"Save the current state of the project as a restorable snapshot (git commit).",
         "inputSchema":{"type":"object","properties":{"label":{"type":"string","description":"Short description of what changed"}},"required":["label"]}},
        {"name":"bump_version","description":"Increase the app version in pubspec.yaml (and Studio).",
         "inputSchema":{"type":"object","properties":{"part":{"type":"string","enum":["major","minor","patch","build"]}},"required":["part"]}},
        {"name":"get_checklist","description":"The publish checklist and which items are done.","inputSchema":no_args},
        {"name":"approve","description":"Internal: asks the user in Studio for permission. Used by Claude Code only.",
         "inputSchema":{"type":"object","properties":{"tool_name":{"type":"string"},"input":{"type":"object"},"tool_use_id":{"type":"string"}},"required":["tool_name","input"]}}
    ])
}

fn text_result(v: &Value, is_error: bool) -> Value {
    let text = if let Some(s) = v.as_str() { s.to_string() } else { serde_json::to_string_pretty(v).unwrap_or_default() };
    json!({"content":[{"type":"text","text":text}],"isError":is_error})
}

fn call_tool(deps: &Deps, project_id: &str, name: &str, args: &Value) -> Value {
    let project = match deps.db.project_get(project_id) {
        Ok(p) => p,
        Err(e) => return text_result(&json!(e.to_string()), true),
    };
    let dir = PathBuf::from(&project.path);
    let arg = |k: &str| args.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let res = match name {
        "project_status" => tools::project_status(&deps.db, &project),
        "list_screens" => tools::list_screens(&dir).map(|s| json!(s)),
        "get_theme" => tools::get_theme(&dir),
        "set_theme" => tools::set_theme(&dir, &arg("seedColor")),
        "preview_status" => Ok(tools::preview_status()),
        "preview_screenshot" => return text_result(&json!("The live preview is not available yet, so there is no screenshot. Ask the user to describe what they see."), true),
        "create_snapshot" => tools::create_snapshot(&deps.db, &project, &arg("label"), "manual"),
        "bump_version" => tools::bump_version(&deps.db, &project, &arg("part")),
        "get_checklist" => tools::get_checklist(&deps.db, &project),
        "approve" => {
            let tool = arg("tool_name");
            let input = args.get("input").cloned().unwrap_or(json!({}));
            let verdict = match deps.perms.request(&deps.db, project_id, &dir, &tool, &input) {
                Decision::Deny => json!({"behavior":"deny","message":"The user declined this action in Studio."}),
                _ => json!({"behavior":"allow","updatedInput":input}),
            };
            // De permissietool moet een JSON-string als tekst teruggeven.
            return json!({"content":[{"type":"text","text":verdict.to_string()}]});
        }
        _ => return text_result(&json!(format!("unknown tool {name}")), true),
    };
    match res {
        Ok(v) => text_result(&v, false),
        Err(e) => text_result(&json!(crate::logging::redact(&e.to_string())), true),
    }
}

/// Eén JSON-RPC-bericht. `None` = melding zonder antwoord.
pub fn handle_rpc(deps: &Deps, project_id: &str, msg: &Value) -> Option<Value> {
    let id = msg.get("id").cloned();
    let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
    let reply = |result: Value| id.clone().map(|id| json!({"jsonrpc":"2.0","id":id,"result":result}));
    match method {
        "initialize" => {
            let ver = msg["params"]["protocolVersion"].as_str().unwrap_or("2025-03-26");
            reply(json!({"protocolVersion":ver,"capabilities":{"tools":{}},"serverInfo":{"name":"studio","version":env!("CARGO_PKG_VERSION")}}))
        }
        "ping" => reply(json!({})),
        "tools/list" => reply(json!({"tools":tool_list()})),
        "tools/call" => {
            let name = msg["params"]["name"].as_str().unwrap_or("");
            reply(call_tool(deps, project_id, name, &msg["params"]["arguments"]))
        }
        m if m.starts_with("notifications/") => None,
        _ => id.map(|id| json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"method not found"}})),
    }
}

// ───────────── HTTP ─────────────

struct Request {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

fn read_request(stream: &TcpStream) -> Option<Request> {
    let mut r = BufReader::new(stream.try_clone().ok()?);
    let mut line = String::new();
    r.read_line(&mut line).ok()?;
    let mut parts = line.split_whitespace();
    let (method, path) = (parts.next()?.to_string(), parts.next()?.to_string());
    let mut headers = vec![];
    let mut total = 0;
    loop {
        let mut h = String::new();
        let n = r.read_line(&mut h).ok()?;
        total += n;
        if n == 0 || total > 16 * 1024 {
            return None;
        }
        if h == "\r\n" || h == "\n" {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            headers.push((k.trim().to_ascii_lowercase(), v.trim().to_string()));
        }
    }
    let len: usize = headers.iter().find(|(k, _)| k == "content-length").and_then(|(_, v)| v.parse().ok()).unwrap_or(0);
    if len > MAX_BODY {
        return None;
    }
    let mut body = vec![0u8; len];
    r.read_exact(&mut body).ok()?;
    Some(Request { method, path, headers, body })
}

fn respond(stream: &mut TcpStream, status: &str, body: &str) {
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n{body}",
        body.len()
    );
}

fn constant_eq(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn handle_conn(mut stream: TcpStream, deps: &Deps, token: &str) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
    let Some(req) = read_request(&stream) else { return respond(&mut stream, "400 Bad Request", "{}") };
    let header = |n: &str| req.headers.iter().find(|(k, _)| k == n).map(|(_, v)| v.as_str()).unwrap_or("");

    let host_ok = header("host").starts_with("127.0.0.1:") || header("host").starts_with("localhost:");
    let auth_ok = header("authorization").strip_prefix("Bearer ").is_some_and(|t| constant_eq(t, token));
    if !host_ok || !auth_ok {
        return respond(&mut stream, "401 Unauthorized", r#"{"error":"unauthorized"}"#);
    }
    let Some(project_id) = req.path.strip_prefix("/mcp/").map(|p| p.trim_end_matches('/')).filter(|p| !p.is_empty()) else {
        return respond(&mut stream, "404 Not Found", "{}");
    };
    if req.method != "POST" {
        return respond(&mut stream, "405 Method Not Allowed", "{}");
    }
    let Ok(msg) = serde_json::from_slice::<Value>(&req.body) else {
        return respond(&mut stream, "400 Bad Request", r#"{"error":"invalid json"}"#);
    };
    // Een batch is een JSON-array; behandel elk bericht apart.
    let out = match &msg {
        Value::Array(items) => {
            let rs: Vec<Value> = items.iter().filter_map(|m| handle_rpc(deps, project_id, m)).collect();
            (!rs.is_empty()).then_some(Value::Array(rs))
        }
        m => handle_rpc(deps, project_id, m),
    };
    match out {
        Some(v) => respond(&mut stream, "200 OK", &v.to_string()),
        None => respond(&mut stream, "202 Accepted", ""),
    }
}

/// Start de server op een vrije poort en geeft poort + token terug.
pub fn start(deps: Deps) -> std::io::Result<McpInfo> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    let token = format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple());
    let deps = Arc::new(deps);
    let t = token.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let (deps, t) = (deps.clone(), t.clone());
            std::thread::spawn(move || handle_conn(stream, &deps, &t));
        }
    });
    Ok(McpInfo { port, token })
}

/// MCP-configuratie voor `claude --mcp-config`: één HTTP-server, gescoped op het project.
pub fn config_json(info: &McpInfo, project_id: &str) -> Value {
    json!({"mcpServers":{crate::engine::claude::MCP_SERVER:{
        "type":"http",
        "url":format!("http://127.0.0.1:{}/mcp/{}", info.port, project_id),
        "headers":{"Authorization":format!("Bearer {}", info.token)}
    }}})
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::NewProject;

    fn deps_with_project() -> (Deps, String, PathBuf) {
        let dir = std::env::temp_dir().join(format!("studio-mcp-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("lib")).unwrap();
        std::fs::write(dir.join("lib/main.dart"), "class HomeScreen extends StatelessWidget {}\n").unwrap();
        let db = Arc::new(Db::open_in_memory().unwrap());
        let p = db
            .project_insert(&NewProject {
                name: "Demo".into(),
                slug: "demo".into(),
                path: dir.to_string_lossy().into(),
                template: "blank".into(),
                description: String::new(),
                bundle_id: "com.example.demo".into(),
                initial_commit: None,
            })
            .unwrap();
        let perms = Arc::new(Permissions::new(Arc::new(|_| {}), Duration::from_millis(100)));
        (Deps { db, perms }, p.id, dir)
    }

    fn rpc(deps: &Deps, pid: &str, method: &str, params: Value) -> Value {
        handle_rpc(deps, pid, &json!({"jsonrpc":"2.0","id":1,"method":method,"params":params})).unwrap()
    }

    fn tool_text(r: &Value) -> String {
        r["result"]["content"][0]["text"].as_str().unwrap().to_string()
    }

    #[test]
    fn handshake_and_tool_list() {
        let (deps, pid, _) = deps_with_project();
        let init = rpc(&deps, &pid, "initialize", json!({"protocolVersion":"2025-06-18"}));
        assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
        assert_eq!(init["result"]["serverInfo"]["name"], "studio");
        assert!(handle_rpc(&deps, &pid, &json!({"jsonrpc":"2.0","method":"notifications/initialized"})).is_none());
        let list = rpc(&deps, &pid, "tools/list", json!({}));
        let names: Vec<&str> = list["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
        for n in ["project_status", "list_screens", "get_theme", "set_theme", "preview_status", "preview_screenshot", "create_snapshot", "bump_version", "get_checklist", "approve"] {
            assert!(names.contains(&n), "missing {n}");
        }
        assert_eq!(rpc(&deps, &pid, "nope", json!({}))["error"]["code"], -32601);
    }

    #[test]
    fn tools_work_on_the_scoped_project() {
        let (deps, pid, dir) = deps_with_project();
        let call = |name: &str, args: Value| rpc(&deps, &pid, "tools/call", json!({"name":name,"arguments":args}));
        let screens = call("list_screens", json!({}));
        assert!(tool_text(&screens).contains("HomeScreen"));
        let theme = call("set_theme", json!({"seedColor":"#0D9488"}));
        assert_eq!(theme["result"]["isError"], false);
        assert!(dir.join("lib/app_theme.dart").exists());
        assert_eq!(call("set_theme", json!({"seedColor":"nope"}))["result"]["isError"], true);
        assert!(tool_text(&call("get_checklist", json!({}))).contains("app_basics"));
        let shot = call("preview_screenshot", json!({}));
        assert_eq!(shot["result"]["isError"], true);
        assert!(tool_text(&call("bogus", json!({}))).contains("unknown tool"));
        // onbekend project
        let r = rpc(&deps, "missing", "tools/call", json!({"name":"project_status","arguments":{}}));
        assert_eq!(r["result"]["isError"], true);
    }

    #[test]
    fn approve_returns_the_permission_prompt_tool_contract() {
        let (deps, pid, _) = deps_with_project();
        // Geen antwoord van de gebruiker binnen de time-out → weigeren.
        let r = rpc(&deps, &pid, "tools/call", json!({"name":"approve","arguments":{"tool_name":"WebFetch","input":{"url":"https://x.dev"}}}));
        let verdict: Value = serde_json::from_str(&tool_text(&r)).unwrap();
        assert_eq!(verdict["behavior"], "deny");
        assert!(verdict["message"].as_str().unwrap().contains("declined"));
    }

    fn http(port: u16, path: &str, auth: Option<&str>, host: &str, body: &str) -> String {
        let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let auth = auth.map(|a| format!("Authorization: Bearer {a}\r\n")).unwrap_or_default();
        write!(s, "POST {path} HTTP/1.1\r\nHost: {host}\r\n{auth}Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len()).unwrap();
        let mut out = String::new();
        s.read_to_string(&mut out).unwrap();
        out
    }

    #[test]
    fn http_layer_requires_token_and_local_host() {
        let (deps, pid, _) = deps_with_project();
        let info = start(deps).unwrap();
        let body = r#"{"jsonrpc":"2.0","id":7,"method":"tools/list"}"#;
        let path = format!("/mcp/{pid}");
        let host = format!("127.0.0.1:{}", info.port);

        assert!(http(info.port, &path, None, &host, body).starts_with("HTTP/1.1 401"));
        assert!(http(info.port, &path, Some("wrong"), &host, body).starts_with("HTTP/1.1 401"));
        assert!(http(info.port, &path, Some(&info.token), "evil.example.com", body).starts_with("HTTP/1.1 401"));
        let ok = http(info.port, &path, Some(&info.token), &host, body);
        assert!(ok.starts_with("HTTP/1.1 200") && ok.contains("create_snapshot") && ok.contains(r#""id":7"#));
        assert!(http(info.port, "/other", Some(&info.token), &host, body).starts_with("HTTP/1.1 404"));
        let note = http(info.port, &path, Some(&info.token), &host, r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#);
        assert!(note.starts_with("HTTP/1.1 202"));
        assert!(http(info.port, &path, Some(&info.token), &host, "{broken").starts_with("HTTP/1.1 400"));

        let cfg = config_json(&info, &pid);
        assert_eq!(cfg["mcpServers"]["studio"]["type"], "http");
        assert!(cfg["mcpServers"]["studio"]["url"].as_str().unwrap().ends_with(&pid));
    }
}

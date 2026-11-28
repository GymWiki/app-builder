//! Lokale MCP-server (JSON-RPC over HTTP) in Studio. Claude Code koppelt via `--mcp-config` en kan
//! zo het project sturen. De server luistert alleen op 127.0.0.1, eist een geheim bearer-token dat
//! Studio bij elke start nieuw maakt, en controleert de Host-header (tegen DNS-rebinding).

use crate::db::Db;
use crate::error::AppError;
use crate::permissions::{Decision, Permissions};
use crate::preview::PreviewManager;
use crate::index::CodeIndex;
use crate::scope::{Level, ScopeRegistry, EDIT_TOOLS};
use crate::screenshot;
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

pub type UiEmit = Arc<dyn Fn(&str, Value) + Send + Sync>;

pub struct Deps {
    pub db: Arc<Db>,
    pub perms: Arc<Permissions>,
    pub preview: Arc<PreviewManager>,
    /// Tijdelijke map voor schermafdrukken.
    pub work_dir: PathBuf,
    /// Het werkgebied van de lopende run per project (fase 7).
    pub scopes: Arc<ScopeRegistry>,
    /// De kluis. De MCP-tools krijgen er alleen STATUS uit, nooit een waarde.
    pub vault: &'static crate::vault::Vault,
    /// Stuurt een gebeurtenis naar de UI (bv. "open het geheim-veld").
    pub emit: UiEmit,
}

pub fn tool_list() -> Value {
    let no_args = json!({"type":"object","properties":{}});
    json!([
        {"name":"project_status","description":"Name, version, status, git state and checklist progress of this app.","inputSchema":no_args},
        {"name":"list_screens","description":"List the screens (widget classes ending in Screen or Page) in lib/.","inputSchema":no_args},
        {"name":"get_theme","description":"Current Studio-managed theme (seed color).","inputSchema":no_args},
        {"name":"set_theme","description":"Set the app's seed color and regenerate lib/app_theme.dart.",
         "inputSchema":{"type":"object","properties":{"seedColor":{"type":"string","description":"Hex color like #4F46E5"}},"required":["seedColor"]}},
        {"name":"preview_status","description":"Whether the live preview is running, its URL and any build/runtime problems.","inputSchema":no_args},
        {"name":"preview_screenshot","description":"Screenshot (PNG) of the running live preview, so you can check your own result.","inputSchema":{"type":"object","properties":{"width":{"type":"integer","description":"Default 390"},"height":{"type":"integer","description":"Default 844"},"region":{"type":"object","description":"Optional box in percent of the screen (x, y, width, height). Returns a second, cropped image of just that box.","properties":{"x":{"type":"number"},"y":{"type":"number"},"width":{"type":"number"},"height":{"type":"number"}}}}}},
        {"name":"create_snapshot","description":"Save the current state of the project as a restorable snapshot (git commit).",
         "inputSchema":{"type":"object","properties":{"label":{"type":"string","description":"Short description of what changed"}},"required":["label"]}},
        {"name":"bump_version","description":"Increase the app version in pubspec.yaml (and Studio).",
         "inputSchema":{"type":"object","properties":{"part":{"type":"string","enum":["major","minor","patch","build"]}},"required":["part"]}},
        {"name":"get_store_listing","description":"The app's store page texts (title, short and full description, category).","inputSchema":no_args},
        {"name":"set_store_listing","description":"Save store page texts. Only the fields you pass are changed. Limits: title 30, shortDescription 80, fullDescription 4000 characters.",
         "inputSchema":{"type":"object","properties":{"title":{"type":"string"},"shortDescription":{"type":"string"},"fullDescription":{"type":"string"},"category":{"type":"string"}}}},
        {"name":"get_checklist","description":"The publish checklist and which items are done.","inputSchema":no_args},
        {"name":"scherm_index","description":"Compact index of all screens: file, route, own files, shared files they use and where they navigate to. Use this instead of searching the project.","inputSchema":no_args},
        {"name":"afhankelijkheden","description":"What a file imports: direct and indirect project files.",
         "inputSchema":{"type":"object","properties":{"file":{"type":"string","description":"Project-relative path, e.g. lib/features/home/home_screen.dart"}},"required":["file"]}},
        {"name":"wie_gebruikt","description":"Who uses a file: direct users, all indirect users and the screens that a change to it can affect.",
         "inputSchema":{"type":"object","properties":{"file":{"type":"string"}},"required":["file"]}},
        {"name":"breid_bereik_uit","description":"Ask for more files to edit when the task really needs a change outside the current scope (shared theme, widgets, models, other screens). Give a short reason; optionally specific files or a level (screen, shared, project). Call this BEFORE editing outside the scope.",
         "inputSchema":{"type":"object","properties":{"reason":{"type":"string"},"files":{"type":"array","items":{"type":"string"}},"level":{"type":"string","enum":["screen","shared","project"]}},"required":["reason"]}},
        {"name":"integratie_status","description":"Status of one integration (apple, google_play, supabase, firebase, local_db, github, vercel): whether it is on, which fields are filled in, which are missing, last test result. Never returns a secret value.",
         "inputSchema":{"type":"object","properties":{"id":{"type":"string"}},"required":["id"]}},
        {"name":"welke_geheimen_ontbreken","description":"Lists the required secrets that are still missing for this app (integration and field only). Never returns a value.","inputSchema":no_args},
        {"name":"vraag_gebruiker_om_geheim","description":"Opens the Secrets page in Studio at the right field so the USER can fill it in. You never see the value and must not ask the user to paste it into the chat.",
         "inputSchema":{"type":"object","properties":{"integratie":{"type":"string"},"veld":{"type":"string"}},"required":["integratie","veld"]}},
        {"name":"open_aankopen_pagina","description":"Opens the Purchases page in Studio with a draft. Use this when the user asks for in-app purchases, a subscription, a paywall or premium features: do NOT write purchase code yourself. Pass what the user said as a proposal; they can adjust and confirm it on the page.",
         "inputSchema":{"type":"object","properties":{"voorstel":{"type":"object","properties":{"description":{"type":"string","description":"What the user wants behind the paywall, in their words"},"products":{"type":"array","items":{"type":"object","properties":{"name":{"type":"string"},"type":{"type":"string","enum":["one_time","subscription","consumable"]},"period":{"type":"string","enum":["week","month","year"]},"price":{"type":"number"},"currency":{"type":"string"},"trialDays":{"type":"integer"},"unlock":{"type":"string"},"description":{"type":"string"}},"required":["name","type"]}}}}},"required":["voorstel"]}},
        {"name":"approve","description":"Internal: asks the user in Studio for permission. Used by Claude Code only.",
         "inputSchema":{"type":"object","properties":{"tool_name":{"type":"string"},"input":{"type":"object"},"tool_use_id":{"type":"string"}},"required":["tool_name","input"]}}
    ])
}

fn text_result(v: &Value, is_error: bool) -> Value {
    let text = if let Some(s) = v.as_str() { s.to_string() } else { serde_json::to_string_pretty(v).unwrap_or_default() };
    let text = crate::scrub::scrub(&text);
    json!({"content":[{"type":"text","text":text}],"isError":is_error})
}

fn call_tool(deps: &Deps, raw_id: &str, name: &str, args: &Value) -> Value {
    // Parallelle bouwtaken (fase 14) gebruiken `<project>~<n>` als eigen run-id zodat elke taak zijn eigen werkgebied heeft; al het andere hoort bij het project.
    let project_id = raw_id.split('~').next().unwrap_or(raw_id);
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
        "preview_status" => {
            let st = deps.preview.state();
            let mine = st.project_id.as_deref() == Some(project_id);
            Ok(if mine {
                json!({ "running": matches!(st.status.as_str(), "running" | "reloading"), "status": st.status, "url": st.url,
                        "problems": st.problems, "hotReloads": st.reloads })
            } else {
                json!({ "running": false, "status": "stopped", "note": "The live preview is not running for this project. Ask the user to press Start in the preview panel." })
            })
        }
        "preview_screenshot" => {
            let st = deps.preview.state();
            let url = match (&st.url, st.project_id.as_deref() == Some(project_id)) {
                (Some(u), true) if st.status != "error" => u.clone(),
                _ => return text_result(&json!("The live preview is not running (or has a build error), so there is no screenshot. Check preview_status, fix errors, or ask the user to start the preview."), true),
            };
            let n = |k: &str, d: u32| args.get(k).and_then(Value::as_u64).map(|v| v as u32).unwrap_or(d);
            return match screenshot::capture(&url, n("width", 390), n("height", 844), 1, &deps.work_dir) {
                Ok(png) => {
                    let f = |k: &str| args.get("region").and_then(|r| r.get(k)).and_then(Value::as_f64);
                    let crop = match (f("x"), f("y"), f("width"), f("height")) {
                        (Some(x), Some(y), Some(w), Some(h)) => screenshot::crop_percent(&png, x, y, w, h).ok(),
                        _ => None,
                    };
                    let mut content = vec![json!({"type":"image","data":screenshot::base64(&png),"mimeType":"image/png"})];
                    let mut note = "Screenshot of the live preview.".to_string();
                    if let Some(c) = crop {
                        content.push(json!({"type":"image","data":screenshot::base64(&c),"mimeType":"image/png"}));
                        note = "Two images: the whole screen, then only the region the user marked.".into();
                    }
                    content.push(json!({"type":"text","text":note}));
                    json!({ "content": content })
                }
                Err(AppError::ToolMissing(_)) => text_result(&json!("No Chrome or Edge browser was found on this computer, so screenshots are unavailable. Ask the user to describe what they see."), true),
                Err(e) => text_result(&json!(crate::logging::redact(&e.to_string())), true),
            };
        }
        "create_snapshot" => tools::create_snapshot(&deps.db, &project, &arg("label"), "manual"),
        "bump_version" => tools::bump_version(&deps.db, &project, &arg("part")),
        "get_checklist" => tools::get_checklist(&deps.db, &project),
        "get_store_listing" => tools::get_store_listing(&dir),
        "set_store_listing" => tools::set_store_listing(&dir, args),
        "integratie_status" => crate::manifest::get(&arg("id"))
            .ok_or_else(|| AppError::NotFound(format!("integration {}", arg("id"))))
            .and_then(|m| deps.vault.status_of(project_id, m))
            .map(|st| json!(st)),
        "welke_geheimen_ontbreken" => {
            let all = deps.vault.missing_secrets(project_id);
            let missing: Vec<Value> = all.iter().filter(|m| !m.field.is_empty()).map(|m| json!({"integration": m.integration, "field": m.field, "integrationOn": m.enabled})).collect();
            let problems: Vec<&str> = all.iter().filter(|m| m.field.is_empty()).map(|m| m.integration.as_str()).collect();
            Ok(json!({ "missing": missing, "needAttention": problems }))
        }
        "vraag_gebruiker_om_geheim" => {
            let (i, f) = (arg("integratie"), arg("veld"));
            match crate::manifest::get(&i).and_then(|m| m.field(&f).map(|fl| (m, fl))) {
                None => Err(AppError::NotFound(format!("field {i}.{f}"))),
                Some((m, _)) => {
                    let on = deps.vault.enabled_ids(project_id).contains(&m.id);
                    (deps.emit)("open-secret", json!({ "projectId": project_id, "integration": i, "field": f, "integrationOn": on }));
                    Ok(json!({ "ok": true, "integrationOn": on, "note": if on { "The user has been asked in Studio. You will not see the value; carry on once they say it is filled in." } else { "This integration is switched off. The user was taken to the Integrations page to switch it on first." } }))
                }
            }
        }
        "open_aankopen_pagina" => {
            let proposal = args.get("voorstel").cloned().unwrap_or(json!({}));
            crate::purchases::load(&dir).and_then(|mut pu| {
                let added = crate::purchases::apply_proposal(&mut pu, &proposal);
                crate::purchases::save(&dir, &pu)?;
                let _ = deps.db.purchases_store(project_id, &serde_json::to_string(&pu).unwrap_or_default());
                (deps.emit)("open-purchases", json!({ "projectId": project_id }));
                Ok(json!({ "ok": true, "added": added, "note": "The user has been taken to the Purchases page in Studio with a draft. They will adjust and confirm it there. Do not write any purchase or paywall code yourself." }))
            })
        }
        "scherm_index" => CodeIndex::build(&dir).map(|i| json!(i.render_markdown())),
        "afhankelijkheden" => CodeIndex::build(&dir).map(|i| {
            let f = arg("file");
            json!({ "file": f, "imports": i.files.get(&f).map(|n| n.imports.clone()).unwrap_or_default(), "allImports": i.deps(&f) })
        }),
        "wie_gebruikt" => CodeIndex::build(&dir).map(|i| {
            let f = arg("file");
            let direct: Vec<&String> = i.files.values().filter(|n| n.imports.contains(&f)).map(|n| &n.path).collect();
            json!({ "file": f, "direct": direct, "all": i.dependents(&f), "screensAffected": i.screens_affected(std::slice::from_ref(&f)) })
        }),
        "breid_bereik_uit" => {
            let files: Vec<String> = args.get("files").and_then(Value::as_array).map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default();
            let level = Level::parse(&arg("level")).filter(|_| !arg("level").is_empty());
            CodeIndex::build(&dir).and_then(|idx| {
                deps.scopes
                    .with(raw_id, |s| {
                        s.expand(&idx, level, &files, &arg("reason"));
                        json!({ "ok": true, "level": s.level.as_str(), "scope": s.describe() })
                    })
                    .ok_or_else(|| AppError::Invalid("there is no active run, so there is no scope to widen".into()))
            })
        }
        "approve" => {
            let tool = arg("tool_name");
            let input = args.get("input").cloned().unwrap_or(json!({}));
            // Bewerkingen worden tegen het werkgebied van de run getoetst, zonder de gebruiker te storen.
            if EDIT_TOOLS.contains(&tool.as_str()) {
                let path = input["file_path"].as_str().or_else(|| input["notebook_path"].as_str()).unwrap_or("").to_string();
                let verdict = deps.scopes.with(raw_id, |s| {
                    if s.permits_path(&dir, &path) {
                        json!({"behavior":"allow","updatedInput":input})
                    } else {
                        s.denied.push(path.clone());
                        json!({"behavior":"deny","message":"This file is outside the scope of the current task. If the task really needs it, call the tool breid_bereik_uit with a short reason first, then try again."})
                    }
                });
                if let Some(v) = verdict {
                    return json!({"content":[{"type":"text","text":v.to_string()}]});
                }
            }
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
#[cfg(test)]
pub fn config_json(info: &McpInfo, project_id: &str) -> Value {
    config_json_with(info, project_id, &json!({}))
}

/// Zelfde, met extra servers (bv. Supabase/Firebase) uit `integrations`.
pub fn config_json_with(info: &McpInfo, project_id: &str, extra: &Value) -> Value {
    let mut servers = json!({crate::engine::claude::MCP_SERVER:{
        "type":"http",
        "url":format!("http://127.0.0.1:{}/mcp/{}", info.port, project_id),
        "headers":{"Authorization":format!("Bearer {}", info.token)}
    }});
    if let (Some(base), Some(add)) = (servers.as_object_mut(), extra.as_object()) {
        for (k, v) in add {
            // Een extra server mag de Studio-server nooit vervangen.
            if k != crate::engine::claude::MCP_SERVER {
                base.insert(k.clone(), v.clone());
            }
        }
    }
    json!({ "mcpServers": servers })
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
                profile_version: 0,
            })
            .unwrap();
        let perms = Arc::new(Permissions::new(Arc::new(|_| {}), Duration::from_millis(100)));
        let preview = Arc::new(PreviewManager::new(Arc::new(|_, _| {})));
        (Deps { db, perms, preview, work_dir: std::env::temp_dir().join("studio-mcp-shots"), scopes: Default::default(), vault: crate::vault::global(), emit: Arc::new(|_, _| {}) }, p.id, dir)
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
        for n in ["project_status", "list_screens", "get_theme", "set_theme", "preview_status", "preview_screenshot", "create_snapshot", "bump_version", "get_checklist", "get_store_listing", "set_store_listing", "approve", "scherm_index", "afhankelijkheden", "wie_gebruikt", "breid_bereik_uit"] {
            assert!(names.contains(&n), "missing {n}");
        }
        assert_eq!(rpc(&deps, &pid, "nope", json!({}))["error"]["code"], -32601);
    }

    /// Deps met een eigen kluis (zelfde database en project), en een opvang voor wat naar de UI wordt gestuurd.
    type Sent = Arc<std::sync::Mutex<Vec<(String, Value)>>>;

    fn deps_with_vault() -> (Deps, String, &'static crate::vault::Vault, Sent) {
        let (v, _, pid, _) = crate::vault::test_vault();
        let db = v.db().clone();
        let vault: &'static crate::vault::Vault = Box::leak(Box::new(v));
        let sent: Sent = Arc::default();
        let sink = sent.clone();
        let deps = Deps {
            db,
            perms: Arc::new(Permissions::new(Arc::new(|_| {}), Duration::from_millis(100))),
            preview: Arc::new(PreviewManager::new(Arc::new(|_, _| {}))),
            work_dir: std::env::temp_dir().join("studio-mcp-shots"),
            scopes: Default::default(),
            vault,
            emit: Arc::new(move |n, p| sink.lock().unwrap().push((n.to_string(), p))),
        };
        (deps, pid, vault, sent)
    }

    #[test]
    fn status_tools_report_fields_but_no_tool_can_ever_return_a_secret_value() {
        let (deps, pid, vault, sent) = deps_with_vault();
        let secrets = [
            "hunter2-keystore-password-XYZ",
            "github_pat_11AAAAAAAAAAAAAAAAAAAA_leakcheckvalue",
            "MIGTAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBHkwdwIBAQQgMCPLEAKLINE01",
            "SERVICEACCOUNTKEYLINE0123456789abcdef",
        ];
        vault.set_text(&pid, "google_play", "keystore_password", secrets[0]).unwrap();
        vault.set_text(&pid, "github", "token", secrets[1]).unwrap();
        vault.set_file(&pid, "apple", "private_key", "AuthKey.p8", format!("-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----\n", secrets[2]).as_bytes()).unwrap();
        let sa = format!(r#"{{"type":"service_account","client_email":"bot@p.iam.gserviceaccount.com","private_key":"-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----\n"}}"#, secrets[3]);
        vault.set_file(&pid, "google_play", "service_account", "sa.json", sa.as_bytes()).unwrap();
        for id in ["google_play", "github", "apple", "google_play"] {
            deps.db.integration_set_enabled(&pid, id, true).unwrap();
        }

        // 1. Er bestaat geen tool met een naam die een waarde belooft.
        let list = tool_list();
        let names: Vec<&str> = list.as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
        for n in &names {
            for bad in ["reveal", "get_secret", "geheim_waarde", "secret_value", "toon_geheim", "export", "vault"] {
                assert!(!n.contains(bad), "tool {n} must not exist");
            }
        }
        // 2. Elke tool aanroepen, met argumenten die iets zouden kunnen opleveren, en de uitvoer op de waarden controleren.
        let args_for = |name: &str| -> Value {
            match name {
                "integratie_status" => json!({"id":"google_play"}),
                "vraag_gebruiker_om_geheim" => json!({"integratie":"github","veld":"token"}),
                "afhankelijkheden" | "wie_gebruikt" => json!({"file":"lib/main.dart"}),
                "approve" => json!({"tool_name":"Read","input":{"file_path":"/x"}}),
                "breid_bereik_uit" => json!({"reason":"x"}),
                "set_theme" => json!({"seedColor":"#112233"}),
                "create_snapshot" => json!({"label":"x"}),
                "bump_version" => json!({"part":"patch"}),
                _ => json!({}),
            }
        };
        let mut all_text = String::new();
        for n in &names {
            let r = handle_rpc(&deps, &pid, &json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":n,"arguments":args_for(n)}})).unwrap();
            all_text.push_str(&r.to_string());
        }
        for sec in secrets {
            assert!(!all_text.contains(sec), "a tool returned a secret value");
        }
        for line in ["MCPLEAKLINE", "SERVICEACCOUNTKEYLINE", "BEGIN PRIVATE KEY"] {
            assert!(!all_text.contains(line), "{line}");
        }
        // 3. De statustools geven wel bruikbare metadata.
        let call = |name: &str, args: Value| -> Value { serde_json::from_str(&tool_text(&rpc(&deps, &pid, "tools/call", json!({"name":name,"arguments":args})))).unwrap() };
        let st = call("integratie_status", json!({"id":"google_play"}));
        assert_eq!((st["state"].as_str(), st["enabled"].as_bool()), (Some("incomplete"), Some(true)), "the signing keystore is still missing");
        assert_eq!(st["fields"][0]["derived"]["client_email"], "bot@p.iam.gserviceaccount.com", "non-secret derived data is fine");
        assert_eq!(st["fields"][0]["present"], true);
        let missing = call("welke_geheimen_ontbreken", json!({}));
        let list: Vec<String> = missing["missing"].as_array().unwrap().iter().map(|m| format!("{}.{}", m["integration"].as_str().unwrap(), m["field"].as_str().unwrap())).collect();
        assert!(list.contains(&"google_play.keystore".to_string()), "{list:?}");
        assert!(!list.contains(&"google_play.service_account".to_string()));
        // 4. Claude kan de gebruiker alleen vragen het in te vullen: de pagina opent, Claude blijft blind.
        sent.lock().unwrap().clear();
        let r = call("vraag_gebruiker_om_geheim", json!({"integratie":"google_play","veld":"keystore"}));
        assert_eq!(r["ok"], true);
        let events = sent.lock().unwrap().clone();
        assert_eq!(events.len(), 1);
        assert_eq!((events[0].0.as_str(), events[0].1["integration"].as_str(), events[0].1["field"].as_str()), ("open-secret", Some("google_play"), Some("keystore")));
        let bad = rpc(&deps, &pid, "tools/call", json!({"name":"vraag_gebruiker_om_geheim","arguments":{"integratie":"nope","veld":"x"}}));
        assert_eq!(bad["result"]["isError"], true);
    }


    #[test]
    fn claude_opens_the_purchases_page_with_a_draft_instead_of_writing_purchase_code() {
        let (deps, pid, dir) = deps_with_project();
        let sent: Arc<std::sync::Mutex<Vec<(String, Value)>>> = Arc::default();
        let s2 = sent.clone();
        let deps = Deps { emit: Arc::new(move |n, v| s2.lock().unwrap().push((n.to_string(), v))), ..deps };
        let call = |name: &str, args: Value| rpc(&deps, &pid, "tools/call", json!({"name":name,"arguments":args}));
        assert!(tool_list().as_array().unwrap().iter().any(|t| t["name"] == "open_aankopen_pagina"));
        let r = call("open_aankopen_pagina", json!({"voorstel":{"description":"Unlimited exports","products":[{"name":"Pro","type":"subscription","period":"year"}]}}));
        assert_eq!(r["result"]["isError"], false, "{r}");
        assert_eq!(serde_json::from_str::<Value>(&tool_text(&r)).unwrap()["added"], 1);
        let saved = crate::purchases::load(&dir).unwrap();
        assert_eq!((saved.products[0].id.as_str(), saved.paywall_description.as_str()), ("pro_yearly", "Unlimited exports"));
        assert_eq!(deps.db.purchases_load(&pid).unwrap().is_some(), true, "the copy for the UI exists too");
        let events = sent.lock().unwrap().clone();
        assert_eq!((events[0].0.as_str(), events[0].1["projectId"].as_str()), ("open-purchases", Some(pid.as_str())));
    }

    #[test]
    fn index_tools_answer_from_the_project_and_scope_gates_edits() {
        let (deps, pid, dir) = deps_with_project();
        let call = |name: &str, args: Value| rpc(&deps, &pid, "tools/call", json!({"name":name,"arguments":args}));
        assert!(tool_text(&call("scherm_index", json!({}))).contains("HomeScreen"));
        let deps_of = tool_text(&call("afhankelijkheden", json!({"file":"lib/main.dart"})));
        assert!(deps_of.contains("\"imports\""));
        assert!(tool_text(&call("wie_gebruikt", json!({"file":"lib/main.dart"}))).contains("screensAffected"));
        // Zonder lopende run is er niets om te verbreden.
        assert_eq!(call("breid_bereik_uit", json!({"reason":"x"}))["result"]["isError"], true);

        // Met een lopend werkgebied: binnen mag, buiten wordt geweigerd tot Claude om meer vraagt.
        let idx = CodeIndex::build(&dir).unwrap();
        let home = idx.screens[0].class.clone();
        let scope = crate::scope::RunScope::new(&idx, &crate::scope::Decision { level: Level::Small, screen: Some(home), why: crate::scope::Why::Selection });
        deps.scopes.set(&pid, scope);
        let own = idx.screens[0].file.clone();
        let verdict = |path: &str| -> Value {
            let r = call("approve", json!({"tool_name":"Edit","input":{"file_path": dir.join(path).to_string_lossy()}}));
            serde_json::from_str(r["result"]["content"][0]["text"].as_str().unwrap()).unwrap()
        };
        assert_eq!(verdict(&own)["behavior"], "allow");
        let denied = verdict("pubspec.yaml");
        assert_eq!(denied["behavior"], "deny");
        assert!(denied["message"].as_str().unwrap().contains("breid_bereik_uit"));
        let r = call("breid_bereik_uit", json!({"reason":"needs a package","files":["pubspec.yaml"]}));
        assert_eq!(r["result"]["isError"], false);
        assert_eq!(verdict("pubspec.yaml")["behavior"], "allow");
        assert_eq!(deps.scopes.with(&pid, |s| (s.denied.len(), s.expansions.len())), Some((1, 1)));
        deps.scopes.take(&pid);
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
        assert!(tool_text(&shot).contains("not running"));
        let ps = call("preview_status", json!({}));
        assert_eq!(ps["result"]["isError"], false);
        assert!(tool_text(&ps).contains("\"running\": false"));
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

    /// Echte Chromium + python3 nodig: `STUDIO_BROWSER=… STUDIO_BROWSER_NO_SANDBOX=1 cargo test real_screenshot_tool -- --ignored`
    #[cfg(unix)]
    #[test]
    #[ignore]
    fn real_screenshot_tool() {
        use std::os::unix::fs::PermissionsExt;
        let (mut deps, pid, dir) = deps_with_project();
        let www = dir.join("www");
        std::fs::create_dir_all(&www).unwrap();
        std::fs::write(www.join("index.html"), "<body style='background:#4f46e5;color:#fff;font:48px sans-serif'>Hello preview</body>").unwrap();
        let fake = dir.join("flutter");
        std::fs::write(&fake, format!("#!/bin/bash\nif [ \"$2\" = \"--help\" ]; then echo '--web-port --web-hostname'; exit 0; fi\nfor a in \"$@\"; do case $a in --web-port=*) port=${{a#--web-port=}};; esac; done\ncd {}\npython3 -m http.server $port --bind 127.0.0.1 >/dev/null 2>&1 &\necho \"lib/main.dart is being served at http://127.0.0.1:$port\"\nwhile IFS= read -r -n1 c; do [ \"$c\" = q ] && kill %1 && exit 0; done\n", www.display())).unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        deps.work_dir = dir.join("shots");
        deps.preview.start(&fake, &pid, &dir, "web").unwrap();
        let end = std::time::Instant::now() + Duration::from_secs(10);
        while deps.preview.state().status != "running" && std::time::Instant::now() < end {
            std::thread::sleep(Duration::from_millis(100));
        }
        let r = rpc(&deps, &pid, "tools/call", json!({"name":"preview_screenshot","arguments":{}}));
        deps.preview.stop();
        let content = &r["result"]["content"];
        assert_eq!(content[0]["type"], "image", "got {r}");
        assert_eq!(content[0]["mimeType"], "image/png");
        assert!(content[0]["data"].as_str().unwrap().starts_with("iVBOR"), "not a PNG");
    }
}

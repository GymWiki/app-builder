//! Koppeling met de Flutter widget inspector via de Dart VM-service, en met Chrome via het DevTools-protocol (CDP).
//!
//! Waarom zo (gemeten, zie DECISIONS.md "Experiment: vak → widget"): de inspector kent geen "welke widget staat op dit punt";
//! de route is selectiemodus aanzetten (`ext.flutter.inspector.show`), een echte tik op dat punt sturen en daarna de selectie
//! uitlezen met `getSelectedSummaryWidget` + `getRootWidgetSummaryTree` (16 + 32 ms; `getParentChain` duurt 5–9 s). De summary-boom
//! bevat precies de widgets uit eigen code (na `addPubRootDirectories`), met aanmaaklocatie. Dit werkt alleen als de app via
//! `flutter run -d chrome` draait (de web-server-variant heeft geen VM-service) en als er maar één app-instantie is.

use crate::error::{AppError, AppResult};
use crate::extent;
use crate::selection::{Link, Selection, Target};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::time::Duration;
use tungstenite::{Message, WebSocket};

/// Blokkerende JSON-RPC-client over een WebSocket; werkt voor de VM-service én voor CDP.
pub struct Rpc {
    ws: WebSocket<TcpStream>,
    next: u64,
    /// Gebeurtenissen die binnenkwamen terwijl we op een antwoord wachtten; `read` geeft ze eerst terug.
    queue: std::collections::VecDeque<Value>,
    /// De VM-service spreekt JSON-RPC 2.0; Chrome (CDP) weigert het extra veld `jsonrpc`.
    jsonrpc: bool,
}

fn io_err(e: impl std::fmt::Display) -> AppError {
    AppError::CommandFailed(format!("preview connection: {e}"))
}

impl Rpc {
    pub fn connect(url: &str, read_timeout: Duration) -> AppResult<Rpc> {
        let rest = url.strip_prefix("ws://").ok_or_else(|| AppError::Invalid("only ws:// is supported".into()))?;
        let host = rest.split('/').next().unwrap_or("");
        let stream = TcpStream::connect_timeout(&host.parse().map_err(io_err)?, Duration::from_secs(3)).map_err(io_err)?;
        stream.set_read_timeout(Some(read_timeout)).ok();
        stream.set_write_timeout(Some(Duration::from_secs(5))).ok();
        stream.set_nodelay(true).ok();
        let (ws, _) = tungstenite::client(url, stream).map_err(io_err)?;
        Ok(Rpc { ws, next: 0, queue: Default::default(), jsonrpc: !url.contains("/devtools/") })
    }

    pub fn send(&mut self, method: &str, params: Value) -> AppResult<u64> {
        self.next += 1;
        let mut msg = json!({ "id": self.next, "method": method, "params": params });
        if self.jsonrpc {
            msg["jsonrpc"] = json!("2.0");
        }
        self.ws.send(Message::Text(msg.to_string().into())).map_err(io_err)?;
        Ok(self.next)
    }

    /// Leest één bericht; `Ok(None)` bij een time-out.
    pub fn read(&mut self) -> AppResult<Option<Value>> {
        if let Some(m) = self.queue.pop_front() {
            return Ok(Some(m));
        }
        match self.ws.read() {
            Ok(Message::Text(t)) => Ok(serde_json::from_str(t.as_str()).ok()),
            Ok(Message::Close(_)) => Err(io_err("closed")),
            Ok(_) => Ok(None),
            Err(tungstenite::Error::Io(e)) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => Ok(None),
            Err(e) => Err(io_err(e)),
        }
    }

    /// Roept een methode aan en wacht op het antwoord (andere berichten worden overgeslagen).
    pub fn call(&mut self, method: &str, params: Value) -> AppResult<Value> {
        let id = self.send(method, params)?;
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        while std::time::Instant::now() < deadline {
            let m = match self.ws.read() {
                Ok(Message::Text(t)) => serde_json::from_str::<Value>(t.as_str()).ok(),
                Ok(Message::Close(_)) => return Err(io_err("closed")),
                Ok(_) => None,
                Err(tungstenite::Error::Io(e)) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => None,
                Err(e) => return Err(io_err(e)),
            };
            if let Some(m) = m {
                if m.get("method").is_some() {
                    self.queue.push_back(m);
                    continue;
                }
                if m.get("id").and_then(Value::as_u64) == Some(id) {
                    if let Some(e) = m.get("error") {
                        return Err(AppError::CommandFailed(format!("{method}: {}", e.to_string().chars().take(200).collect::<String>())));
                    }
                    return Ok(m.get("result").cloned().unwrap_or(Value::Null));
                }
            }
        }
        Err(AppError::CommandFailed(format!("{method}: no answer")))
    }

    pub fn close(mut self) {
        let _ = self.ws.close(None);
    }
}

/// Eenvoudige `GET` zonder extra crate (alleen voor `http://127.0.0.1:<poort>/json/list`).
pub fn http_get(port: u16, path: &str) -> AppResult<String> {
    let mut s = TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), Duration::from_secs(3)).map_err(io_err)?;
    s.set_read_timeout(Some(Duration::from_secs(5))).ok();
    s.write_all(format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\nAccept: */*\r\n\r\n").as_bytes()).map_err(io_err)?;
    // Chrome sluit de verbinding niet na het antwoord: lees de kop en daarna precies `Content-Length` bytes.
    let mut buf: Vec<u8> = vec![];
    let mut chunk = [0u8; 4096];
    loop {
        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&buf[..pos]).to_lowercase();
            let want = head.lines().find_map(|l| l.strip_prefix("content-length:")).and_then(|v| v.trim().parse::<usize>().ok());
            let have = buf.len() - (pos + 4);
            if want.is_none_or(|w| have >= w) {
                return Ok(String::from_utf8_lossy(&buf[pos + 4..]).to_string());
            }
        }
        match s.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
            Err(e) => return Err(io_err(e)),
        }
    }
    Ok(buf.windows(4).position(|w| w == b"\r\n\r\n").map(|p| String::from_utf8_lossy(&buf[p + 4..]).to_string()).unwrap_or_default())
}

/// De pagina van de app in de Chrome van `flutter run`: (WebSocket-URL, pagina-URL).
pub fn page_target(cdp_port: u16, web_port: u16) -> AppResult<String> {
    let body = http_get(cdp_port, "/json/list")?;
    let list: Vec<Value> = serde_json::from_str(&body).map_err(io_err)?;
    let prefix = format!("http://127.0.0.1:{web_port}");
    list.iter()
        .find(|t| t["type"] == "page" && t["url"].as_str().is_some_and(|u| u.starts_with(&prefix)))
        .and_then(|t| t["webSocketDebuggerUrl"].as_str().map(String::from))
        .ok_or_else(|| AppError::NotFound("preview page".into()))
}

// ───────────── widgets ─────────────

/// Eén widget uit de summary-boom (eigen code).
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub widget: String,
    pub file: String, // relatief aan de projectmap
    pub line: usize,
    pub column: usize,
    pub text: Option<String>,
}

fn rel_file(project_dir: &Path, uri: &str) -> Option<String> {
    let p = uri.strip_prefix("file://").unwrap_or(uri);
    let root = project_dir.to_string_lossy();
    p.strip_prefix(root.as_ref()).map(|r| r.trim_start_matches('/').to_string())
}

fn first_text(n: &Value) -> Option<String> {
    if let Some(t) = n["textPreview"].as_str().filter(|t| !t.is_empty()) {
        return Some(t.to_string());
    }
    n["children"].as_array()?.iter().find_map(first_text)
}

/// Pad (buiten → binnen) in de summary-boom naar de node met `id`, alleen eigen code.
pub fn path_to(tree: &Value, id: &str, project_dir: &Path) -> Vec<Node> {
    fn walk<'a>(n: &'a Value, id: &str, acc: &mut Vec<&'a Value>) -> bool {
        acc.push(n);
        if n["valueId"] == id || n["children"].as_array().is_some_and(|c| c.iter().any(|ch| walk(ch, id, acc))) {
            return true;
        }
        acc.pop();
        false
    }
    let mut acc = vec![];
    if !walk(tree, id, &mut acc) {
        return vec![];
    }
    acc.into_iter()
        .filter(|n| n["createdByLocalProject"] == true)
        .filter_map(|n| {
            let loc = &n["creationLocation"];
            Some(Node {
                widget: n["description"].as_str()?.to_string(),
                file: rel_file(project_dir, loc["file"].as_str()?)?,
                line: loc["line"].as_u64()? as usize,
                column: loc["column"].as_u64()? as usize,
                text: first_text(n),
            })
        })
        .collect()
}

pub fn is_shared(file: &str) -> bool {
    file.starts_with("lib/core/") || file.starts_with("lib/shared/")
}

/// Bouwt een doel uit het pad (buiten → binnen): het diepste niveau buiten gedeelde componenten, met de hele keten.
pub fn target_from_path(path: &[Node], project_dir: &Path, screens: &[(String, String)]) -> Option<(Target, Option<(String, String)>)> {
    let inner_first: Vec<&Node> = path.iter().rev().collect();
    let call_site = inner_first.iter().find(|n| !is_shared(&n.file))?;
    let via_shared = inner_first.first().is_some_and(|n| is_shared(&n.file));
    let src = std::fs::read_to_string(project_dir.join(&call_site.file)).unwrap_or_default();
    let span = extent::expression_span(&src, call_site.line, call_site.column);
    let screen = inner_first.iter().find_map(|n| screens.iter().find(|(c, _)| *c == n.widget).cloned());
    // Tekst: eerst die van het gekozen element zelf, anders van de binnenste node met tekst.
    let text = call_site.text.clone().or_else(|| inner_first.iter().find_map(|n| n.text.clone()));
    let chain = inner_first.iter().skip_while(|n| *n != call_site).map(|n| Link { widget: n.widget.clone(), file: n.file.clone(), line: n.line, column: n.column }).collect();
    Some((
        Target {
            widget: call_site.widget.clone(),
            text,
            file: call_site.file.clone(),
            start_line: span.map(|s| s.start_line).unwrap_or(call_site.line),
            end_line: span.map(|s| s.end_line).unwrap_or(call_site.line),
            chain,
            via_shared,
        },
        screen,
    ))
}

/// Meetpunten in een vak (procenten): midden, hoeken naar binnen en een raster.
pub fn sample_points(b: [f64; 4]) -> Vec<(f64, f64)> {
    let [x, y, w, h] = b;
    let mut pts = vec![];
    let n = if w * h > 1500.0 { 4 } else { 3 };
    for i in 0..n {
        for j in 0..n {
            pts.push((x + w * (i as f64 + 0.5) / n as f64, y + h * (j as f64 + 0.5) / n as f64));
        }
    }
    pts
}

/// Zet een vak (procenten) om in een selectie via de inspector. `viewport` = de afmetingen in CSS-pixels.
pub fn select_box(vm_ws: &str, page_ws: &str, project_dir: &Path, viewport: (u32, u32), bbox: [f64; 4], screens: &[(String, String)]) -> AppResult<Selection> {
    let mut vm = Rpc::connect(vm_ws, Duration::from_millis(500))?;
    // Na een hot restart is het isolate-id nieuw: telkens opnieuw opvragen.
    let iso = vm.call("getVM", json!({}))?["isolates"][0]["id"].as_str().ok_or_else(|| AppError::NotFound("isolate".into()))?.to_string();
    let ext = |vm: &mut Rpc, name: &str, args: Value| -> AppResult<Value> {
        let mut p = json!({ "isolateId": iso });
        if let (Some(o), Some(a)) = (p.as_object_mut(), args.as_object()) {
            o.extend(a.clone());
        }
        vm.call(&format!("ext.flutter.inspector.{name}"), p)
    };
    ext(&mut vm, "addPubRootDirectories", json!({ "arg0": project_dir.to_string_lossy() }))?;
    let mut page = Rpc::connect(page_ws, Duration::from_millis(500))?;
    ext(&mut vm, "show", json!({ "enabled": "true" }))?;
    std::thread::sleep(Duration::from_millis(200));
    let group = format!("studio-{}", uuid::Uuid::new_v4().simple());
    let mut targets: Vec<Target> = vec![];
    let mut screen: Option<(String, String)> = None;
    let mut result = Ok(());
    for (px, py) in sample_points(bbox) {
        let (x, y) = (px / 100.0 * viewport.0 as f64, py / 100.0 * viewport.1 as f64);
        let step = (|| -> AppResult<()> {
            for ty in ["mousePressed", "mouseReleased"] {
                page.call("Input.dispatchMouseEvent", json!({ "type": ty, "x": x, "y": y, "button": "left", "clickCount": 1 }))?;
            }
            std::thread::sleep(Duration::from_millis(150));
            let sel = ext(&mut vm, "getSelectedSummaryWidget", json!({ "previousSelectionId": "", "objectGroup": group }))?;
            let Some(id) = sel["result"]["valueId"].as_str() else { return Ok(()) };
            let tree = ext(&mut vm, "getRootWidgetSummaryTree", json!({ "objectGroup": group }))?;
            let path = path_to(&tree["result"], id, project_dir);
            if let Some((t, s)) = target_from_path(&path, project_dir, screens) {
                if screen.is_none() {
                    screen = s;
                }
                if !targets.iter().any(|o| o.file == t.file && o.start_line == t.start_line && o.widget == t.widget) {
                    targets.push(t);
                }
            }
            Ok(())
        })();
        if let Err(e) = step {
            result = Err(e);
            break;
        }
    }
    let _ = ext(&mut vm, "show", json!({ "enabled": "false" }));
    let _ = ext(&mut vm, "disposeGroup", json!({ "objectGroup": group }));
    vm.close();
    page.close();
    result?;
    // Een omvattende widget (bv. de Column om alles heen) vervalt zodra er een widget in zit die ook gekozen is.
    let all = targets.clone();
    targets.retain(|t| !all.iter().any(|o| o != t && o.chain.iter().skip(1).any(|l| l.file == t.file && l.widget == t.widget && l.line == t.start_line)));
    if targets.is_empty() {
        return Err(AppError::NotFound("widgets in the selected box".into()));
    }
    Ok(Selection { screen: screen.as_ref().map(|s| s.0.clone()), screen_file: screen.map(|s| s.1), targets, bbox: Some(bbox) })
}

/// Zet de browser op het formaat van de gekozen telefoon (alleen op de verbinding die het scherm doorstuurt).
pub fn viewport_params(w: u32, h: u32) -> Value {
    json!({ "width": w, "height": h, "deviceScaleFactor": 2, "mobile": false })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(desc: &str, file: &str, line: u64, id: &str, children: Vec<Value>, text: Option<&str>) -> Value {
        let mut v = json!({ "description": desc, "valueId": id, "createdByLocalProject": true, "creationLocation": { "file": format!("file:///p/{file}"), "line": line, "column": 5 }, "children": children });
        if let Some(t) = text {
            v["textPreview"] = json!(t);
        }
        v
    }

    fn tree() -> Value {
        json!({ "description": "[root]", "valueId": "r", "children": [
            node("HomeScreen", "lib/main.dart", 14, "a", vec![
                node("Column", "lib/home_screen.dart", 15, "b", vec![
                    node("PrimaryButton", "lib/home_screen.dart", 22, "c", vec![
                        node("ElevatedButton", "lib/core/primary_button.dart", 11, "d", vec![node("Text", "lib/core/primary_button.dart", 13, "e", vec![], Some("Open list"))], None)], None),
                    node("Text", "lib/home_screen.dart", 18, "f", vec![], Some("Welcome")),
                ], None)], None)] })
    }

    #[test]
    fn the_path_keeps_only_user_code_and_resolves_shared_components_to_the_call_site() {
        let dir = Path::new("/p");
        let path = path_to(&tree(), "e", dir);
        assert_eq!(path.iter().map(|n| n.widget.as_str()).collect::<Vec<_>>(), ["HomeScreen", "Column", "PrimaryButton", "ElevatedButton", "Text"], "[root] has no createdByLocalProject");
        let screens = vec![("HomeScreen".to_string(), "lib/home_screen.dart".to_string())];
        let (t, s) = target_from_path(&path, dir, &screens).unwrap();
        assert_eq!((t.widget.as_str(), t.file.as_str(), t.start_line, t.via_shared), ("PrimaryButton", "lib/home_screen.dart", 22, true));
        assert_eq!(t.text.as_deref(), Some("Open list"), "text comes from the shared Text inside");
        assert_eq!(s, Some(("HomeScreen".into(), "lib/home_screen.dart".into())));
        assert_eq!(t.chain.iter().map(|l| l.widget.as_str()).collect::<Vec<_>>(), ["PrimaryButton", "Column", "HomeScreen"]);
        assert!(path_to(&tree(), "zzz", dir).is_empty());
    }

    #[test]
    fn a_plain_widget_is_its_own_target() {
        let dir = Path::new("/p");
        let path = path_to(&tree(), "f", dir);
        let (t, _) = target_from_path(&path, dir, &[]).unwrap();
        assert_eq!((t.widget.as_str(), t.via_shared, t.text.as_deref()), ("Text", false, Some("Welcome")));
    }

    #[test]
    fn sample_points_stay_inside_the_box() {
        for b in [[10.0, 20.0, 30.0, 8.0], [0.0, 0.0, 100.0, 100.0]] {
            let pts = sample_points(b);
            assert!(pts.len() >= 9 && pts.iter().all(|(x, y)| *x > b[0] && *x < b[0] + b[2] && *y > b[1] && *y < b[1] + b[3]));
        }
    }
}

#[cfg(test)]
mod real_tests {
    use super::*;

    /// Handmatige controle tegen een draaiende `flutter run -d chrome` (poorten uit de omgeving).
    #[test]
    #[ignore]
    fn real_cdp_page_and_frames() {
        let (cdp, web): (u16, u16) = (std::env::var("CDP_PORT").unwrap().parse().unwrap(), std::env::var("WEB_PORT").unwrap().parse().unwrap());
        let url = page_target(cdp, web).expect("page target");
        println!("page ws: {url}");
        let mut rpc = Rpc::connect(&url, Duration::from_millis(200)).expect("connect");
        println!("Page.enable: {:?}", rpc.call("Page.enable", json!({})));
        println!("viewport: {:?}", rpc.call("Emulation.setDeviceMetricsOverride", viewport_params(500, 644)));
        println!("start: {:?}", rpc.call("Page.startScreencast", json!({ "format": "jpeg", "quality": 70, "everyNthFrame": 1 })));
        let end = std::time::Instant::now() + Duration::from_secs(10);
        let mut n = 0;
        while std::time::Instant::now() < end && n < 3 {
            if let Ok(Some(m)) = rpc.read() {
                if m["method"] == "Page.screencastFrame" {
                    n += 1;
                    println!("frame {n}: {} bytes, meta {}", m["params"]["data"].as_str().map(str::len).unwrap_or(0), m["params"]["metadata"]);
                    let _ = rpc.send("Page.screencastFrameAck", json!({ "sessionId": m["params"]["sessionId"] }));
                }
            }
        }
        assert!(n >= 1, "frames arrive");
    }
}

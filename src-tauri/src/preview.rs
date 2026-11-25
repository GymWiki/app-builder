//! Live preview: `flutter run` als subproces (web-server of een echt apparaat), met
//! hot restart/reload na elke wijziging in `lib/` (via `notify`), en het opvangen van
//! bouwfouten zodat de UI ze met één klik naar Claude kan sturen.
//!
//! De uitvoer van `flutter run` wordt als tekst gelezen (geen `--machine`); de herkenning zit in
//! `Inner::feed_line` en is getest met voorbeelduitvoer. Voor het echte gedrag per Flutter-versie
//! blijft het ruwe log altijd beschikbaar.

use crate::error::{AppError, AppResult};
use crate::proc;
use notify::{RecursiveMode, Watcher};
use serde::Serialize;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Stdio};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const LOG_MAX: usize = 400;
const EXCERPT_LINES: usize = 14;

pub type EmitFn = Arc<dyn Fn(&str, serde_json::Value) + Send + Sync>;

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct Problem {
    pub file: Option<String>,
    pub line: Option<u32>,
    pub message: String,
    /// De regels eromheen (voor "Laat Claude dit oplossen").
    pub excerpt: String,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PreviewState {
    pub project_id: Option<String>,
    /// stopped | starting | running | reloading | error
    pub status: String,
    /// "web" of een apparaat-id.
    pub target: String,
    pub url: Option<String>,
    pub problems: Vec<Problem>,
    pub reloads: u32,
}

#[derive(Default)]
pub struct Inner {
    pub state: PreviewState,
    pub log: Vec<String>,
    capture: Option<(usize, usize)>, // (probleemindex, resterende regels)
}

pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for n in chars.by_ref() {
                    if n.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else if c != '\r' {
            out.push(c);
        }
    }
    out
}

/// `lib/main.dart:12:5: Error: …` → (bestand, regel, boodschap)
pub fn parse_compile_error(line: &str) -> Option<(String, u32, String)> {
    let (loc, msg) = line.split_once(": Error: ")?;
    let mut parts = loc.rsplitn(3, ':');
    let _col = parts.next()?.trim().parse::<u32>().ok()?;
    let ln = parts.next()?.trim().parse::<u32>().ok()?;
    let file = parts.next()?.trim();
    if !file.ends_with(".dart") {
        return None;
    }
    Some((file.to_string(), ln, msg.trim().to_string()))
}

/// Echte uitvoer van `flutter run -d web-server` (Flutter 3.50): `Performing hot restart...   71ms`. De regel zonder
/// tijd is "bezig", met tijd is "klaar". Native apparaten melden `Reloaded N of M libraries in Xms`.
fn is_finished_reload(line: &str) -> bool {
    let t = line.trim_end();
    (t.contains("Performing hot restart") || t.contains("Performing hot reload")) && t.strip_suffix("ms").and_then(|r| r.chars().last()).is_some_and(|c| c.is_ascii_digit())
}

impl Inner {
    fn set_status(&mut self, s: &str) {
        self.state.status = s.to_string();
    }

    /// Verwerkt één regel uitvoer. Geeft `true` als de zichtbare staat veranderde.
    pub fn feed_line(&mut self, raw: &str) -> bool {
        let line = strip_ansi(raw);
        let line = line.trim_end();
        if line.trim().is_empty() {
            return false;
        }
        self.log.push(crate::logging::redact(line));
        if self.log.len() > LOG_MAX {
            self.log.drain(..self.log.len() - LOG_MAX);
        }
        let before = (self.state.status.clone(), self.state.problems.len(), self.state.url.clone(), self.state.reloads);

        // Extra regels bij een eerder gevonden probleem verzamelen.
        if let Some((i, left)) = self.capture {
            if left > 0 && !line.starts_with("═") {
                if let Some(p) = self.state.problems.get_mut(i) {
                    p.excerpt.push('\n');
                    p.excerpt.push_str(line);
                }
                self.capture = Some((i, left - 1));
            } else {
                self.capture = None;
            }
        }

        if line.starts_with("Launching ") {
            self.state.problems.clear();
            self.set_status("starting");
        } else if let Some(idx) = line.find(" is being served at ") {
            let url = line[idx + 20..].trim().to_string();
            self.state.url = Some(url);
            self.state.problems.clear();
            self.set_status("running");
        } else if is_finished_reload(line) && !self.state.problems.is_empty() {
            // Echte uitvoer: de compilatiefouten staan VOOR "Performing hot restart... 123ms". Die herstart is mislukt,
            // dus de fouten moeten blijven staan.
            self.state.reloads += 1;
            self.set_status("error");
        } else if line.contains("Performing hot re") && !is_finished_reload(line) {
            // Begin van een nieuwe poging: oude fouten vervallen.
            self.state.problems.clear();
            self.set_status("reloading");
        } else if line.contains("Restarted application in") || line.starts_with("Reloaded ") || is_finished_reload(line) {
            self.state.problems.clear();
            self.state.reloads += 1;
            self.set_status("running");
        } else if line.contains("No client connected") {
            // Nog geen browser verbonden (in Studio is dat het iframe): niets kapot, wel niets te herladen.
            if self.state.status == "reloading" {
                self.set_status("running");
            }
        } else if let Some((file, ln, msg)) = parse_compile_error(line) {
            self.state.problems.push(Problem { file: Some(file), line: Some(ln), message: msg, excerpt: line.to_string() });
            self.capture = Some((self.state.problems.len() - 1, EXCERPT_LINES));
            self.set_status("error");
        } else if line.contains("EXCEPTION CAUGHT") {
            self.state.problems.push(Problem { file: None, line: None, message: line.trim_matches(['═', '╡', '╞', ' ']).to_string(), excerpt: line.to_string() });
            self.capture = Some((self.state.problems.len() - 1, EXCERPT_LINES));
        } else if line.contains("Failed to compile application") || line.contains("Try again after fixing the above error") {
            if self.state.problems.is_empty() {
                self.state.problems.push(Problem { message: line.to_string(), excerpt: line.to_string(), ..Default::default() });
            }
            self.set_status("error");
        } else if line.starts_with("Error: ") || line.starts_with("Error ") {
            self.state.problems.push(Problem { message: line.to_string(), excerpt: line.to_string(), ..Default::default() });
            self.set_status("error");
        }
        before != (self.state.status.clone(), self.state.problems.len(), self.state.url.clone(), self.state.reloads)
    }
}

// ───────────── proces ─────────────

struct Running {
    child: Arc<Mutex<Child>>,
    stdin: Arc<Mutex<Option<ChildStdin>>>,
    stopping: Arc<std::sync::atomic::AtomicBool>,
    _watcher: Option<notify::RecommendedWatcher>,
    web: bool,
}

pub struct PreviewManager {
    inner: Arc<Mutex<Inner>>,
    run: Mutex<Option<Running>>,
    emit: EmitFn,
}

fn free_port() -> AppResult<u16> {
    Ok(std::net::TcpListener::bind("127.0.0.1:0")?.local_addr()?.port())
}

fn kill_tree(child: &Arc<Mutex<Child>>, stdin: &Arc<Mutex<Option<ChildStdin>>>) {
    // 1. netjes afsluiten ('q'), 2. de hele processboom beëindigen (flutter.bat start dart als kindproces).
    if let Some(mut s) = stdin.lock().unwrap_or_else(|e| e.into_inner()).take() {
        let _ = s.write_all(b"q");
        let _ = s.flush();
    }
    let deadline = Instant::now() + Duration::from_millis(1500);
    while Instant::now() < deadline {
        if child.lock().unwrap_or_else(|e| e.into_inner()).try_wait().ok().flatten().is_some() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let pid = child.lock().unwrap_or_else(|e| e.into_inner()).id();
    #[cfg(windows)]
    {
        let mut c = std::process::Command::new("taskkill");
        c.args(["/PID", &pid.to_string(), "/T", "/F"]);
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
        let _ = c.output();
    }
    #[cfg(unix)]
    {
        for sig in ["-TERM", "-KILL"] {
            let _ = std::process::Command::new("kill").args([sig, "--", &format!("-{pid}")]).output();
            std::thread::sleep(Duration::from_millis(200));
        }
    }
    let mut c = child.lock().unwrap_or_else(|e| e.into_inner());
    let _ = c.kill();
    let _ = c.wait();
}

impl PreviewManager {
    pub fn new(emit: EmitFn) -> Self {
        Self { inner: Arc::new(Mutex::new(Inner { state: PreviewState { status: "stopped".into(), target: "web".into(), ..Default::default() }, ..Default::default() })), run: Mutex::new(None), emit }
    }

    pub fn state(&self) -> PreviewState {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).state.clone()
    }

    pub fn log(&self) -> Vec<String> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).log.clone()
    }

    fn publish(&self) {
        (self.emit)("preview-state", serde_json::to_value(self.state()).unwrap_or_default());
    }

    pub fn stop(&self) {
        let running = self.run.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(r) = running {
            r.stopping.store(true, std::sync::atomic::Ordering::SeqCst);
            kill_tree(&r.child, &r.stdin);
        }
        {
            let mut i = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            i.state.status = "stopped".into();
            i.state.url = None;
        }
        self.publish();
    }

    /// Start `flutter run` voor een project. `target` = "web" of een apparaat-id uit `flutter devices`.
    pub fn start(&self, flutter: &Path, project_id: &str, dir: &Path, target: &str) -> AppResult<()> {
        self.stop();
        let web = target == "web";
        let help = proc::run(flutter, &["run", "--help"], Some(dir), Duration::from_secs(120))?.combined();
        let mut args: Vec<String> = vec!["run".into()];
        let mut url = None;
        if web {
            for f in ["--web-port", "--web-hostname"] {
                if !help.contains(f) {
                    return Err(AppError::CommandFailed(format!("outdated flutter: `flutter run` has no {f}")));
                }
            }
            let port = free_port()?;
            args.extend(["-d".into(), "web-server".into(), "--web-hostname=127.0.0.1".into(), format!("--web-port={port}")]);
            url = Some(format!("http://127.0.0.1:{port}"));
        } else {
            if !target.chars().all(|c| c.is_ascii_alphanumeric() || "-_.:".contains(c)) {
                return Err(AppError::Invalid("invalid device id".into()));
            }
            args.extend(["-d".into(), target.to_string()]);
        }

        {
            let mut i = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            *i = Inner { state: PreviewState { project_id: Some(project_id.into()), status: "starting".into(), target: target.into(), url: None, problems: vec![], reloads: 0 }, ..Default::default() };
        }
        self.publish();

        let mut cmd = proc::command(flutter);
        cmd.args(&args).current_dir(dir).stdin(Stdio::piped());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0); // eigen groep, zodat we de hele boom kunnen beëindigen
        }
        let mut child = cmd.spawn().map_err(|e| AppError::CommandFailed(format!("flutter run: {e}")))?;
        let stdin = Arc::new(Mutex::new(child.stdin.take()));
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let child = Arc::new(Mutex::new(child));
        let stopping = Arc::new(std::sync::atomic::AtomicBool::new(false));

        let (done_tx, done_rx) = channel::<()>();
        for stream in [stdout.map(|s| Box::new(s) as Box<dyn std::io::Read + Send>), stderr.map(|s| Box::new(s) as Box<dyn std::io::Read + Send>)].into_iter().flatten() {
            let (inner, emit, tx) = (self.inner.clone(), self.emit.clone(), done_tx.clone());
            std::thread::spawn(move || {
                for line in BufReader::new(stream).lines().map_while(Result::ok) {
                    let (changed, st) = {
                        let mut i = inner.lock().unwrap_or_else(|e| e.into_inner());
                        let c = i.feed_line(&line);
                        (c, i.state.clone())
                    };
                    if changed {
                        emit("preview-state", serde_json::to_value(st).unwrap_or_default());
                    }
                }
                let _ = tx.send(());
            });
        }
        drop(done_tx);

        // Wacht op het einde van het proces en meld dat (ook als flutter zelf crasht).
        {
            let (inner, emit, child, stopping) = (self.inner.clone(), self.emit.clone(), child.clone(), stopping.clone());
            std::thread::spawn(move || {
                let _ = done_rx.recv();
                let _ = done_rx.recv();
                let code = child.lock().unwrap_or_else(|e| e.into_inner()).wait().ok().and_then(|s| s.code());
                if stopping.load(std::sync::atomic::Ordering::SeqCst) {
                    return;
                }
                let st = {
                    let mut i = inner.lock().unwrap_or_else(|e| e.into_inner());
                    if code != Some(0) && i.state.problems.is_empty() {
                        let excerpt = i.log.iter().rev().take(10).rev().cloned().collect::<Vec<_>>().join("\n");
                        let message = format!("flutter run stopped unexpectedly (exit code {})", code.map(|c| c.to_string()).unwrap_or("?".into()));
                        i.state.problems.push(Problem { message, excerpt, ..Default::default() });
                        i.state.status = "error".into();
                    } else if i.state.status != "error" {
                        i.state.status = "stopped".into();
                    }
                    i.state.url = None;
                    i.state.clone()
                };
                emit("preview-state", serde_json::to_value(st).unwrap_or_default());
            });
        }

        // Webserver: ook "running" zodra de poort echt antwoordt (los van de tekst van de log).
        if let Some(u) = url.clone() {
            let (inner, emit, stopping) = (self.inner.clone(), self.emit.clone(), stopping.clone());
            let port: u16 = u.rsplit(':').next().and_then(|p| p.parse().ok()).unwrap_or(0);
            std::thread::spawn(move || {
                while !stopping.load(std::sync::atomic::Ordering::SeqCst) {
                    std::thread::sleep(Duration::from_millis(500));
                    let st = {
                        let mut i = inner.lock().unwrap_or_else(|e| e.into_inner());
                        if i.state.status == "stopped" {
                            return;
                        }
                        if i.state.status == "error" || i.state.status == "running" || i.state.status == "reloading" {
                            if i.state.status != "error" && i.state.url.is_none() {
                                i.state.url = Some(u.clone());
                            } else {
                                return;
                            }
                        } else if std::net::TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), Duration::from_millis(300)).is_ok() {
                            i.state.url = Some(u.clone());
                            i.state.status = "running".into();
                        } else {
                            continue;
                        }
                        i.state.clone()
                    };
                    emit("preview-state", serde_json::to_value(st).unwrap_or_default());
                    return;
                }
            });
        }

        // Bestanden in lib/ bekijken: elke wijziging (van Claude of handmatig) herlaadt de preview.
        let watcher = self.watch(dir, web, &stdin);
        *self.run.lock().unwrap_or_else(|e| e.into_inner()) = Some(Running { child, stdin, stopping, _watcher: watcher, web });
        Ok(())
    }

    fn watch(&self, dir: &Path, web: bool, stdin: &Arc<Mutex<Option<ChildStdin>>>) -> Option<notify::RecommendedWatcher> {
        let (tx, rx) = channel::<()>();
        let mut w = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            if let Ok(ev) = res {
                if ev.paths.iter().any(|p| p.extension().is_some_and(|e| e == "dart")) && !matches!(ev.kind, notify::EventKind::Access(_)) {
                    let _ = tx.send(());
                }
            }
        })
        .ok()?;
        w.watch(&dir.join("lib"), RecursiveMode::Recursive).ok()?;
        let (stdin, inner, emit) = (stdin.clone(), self.inner.clone(), self.emit.clone());
        std::thread::spawn(move || {
            while rx.recv().is_ok() {
                // Debounce: een AI-beurt schrijft vaak meerdere bestanden snel na elkaar.
                std::thread::sleep(Duration::from_millis(500));
                while rx.try_recv().is_ok() {}
                let ok = {
                    let mut g = stdin.lock().unwrap_or_else(|e| e.into_inner());
                    match g.as_mut() {
                        // Web: hot restart (R) is betrouwbaar; apparaten: hot reload (r).
                        Some(s) => s.write_all(if web { b"R" } else { b"r" }).and_then(|_| s.flush()).is_ok(),
                        None => false,
                    }
                };
                if ok {
                    let st = {
                        let mut i = inner.lock().unwrap_or_else(|e| e.into_inner());
                        if i.state.status == "running" {
                            i.state.status = "reloading".into();
                        }
                        i.state.clone()
                    };
                    emit("preview-state", serde_json::to_value(st).unwrap_or_default());
                }
            }
        });
        Some(w)
    }

    /// Handmatig herladen (knop "Vernieuwen").
    pub fn reload(&self) -> AppResult<()> {
        let run = self.run.lock().unwrap_or_else(|e| e.into_inner());
        let r = run.as_ref().ok_or_else(|| AppError::NotFound("running preview".into()))?;
        let mut g = r.stdin.lock().unwrap_or_else(|e| e.into_inner());
        let s = g.as_mut().ok_or_else(|| AppError::NotFound("running preview".into()))?;
        s.write_all(if r.web { b"R" } else { b"r" })?;
        s.flush()?;
        Ok(())
    }
}

/// Aangesloten apparaten uit `flutter devices --machine`.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub id: String,
    pub name: String,
    pub platform: String,
    pub emulator: bool,
    pub wireless: bool,
}

pub fn parse_devices(json: &str) -> Vec<Device> {
    let start = json.find('[').unwrap_or(0);
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json[start..].trim()) else { return vec![] };
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|d| {
            let id = d["id"].as_str()?.to_string();
            let platform = d["targetPlatform"].as_str().unwrap_or("").to_string();
            // Alleen echte mobiele apparaten en emulators; "web-server"/chrome/desktop zijn al de standaardpreview.
            if !(platform.starts_with("android") || platform.starts_with("ios")) {
                return None;
            }
            Some(Device {
                id,
                name: d["name"].as_str().unwrap_or("device").to_string(),
                platform,
                emulator: d["emulator"].as_bool().unwrap_or(false),
                wireless: d["connectionType"].as_str() == Some("wireless") || d["id"].as_str().is_some_and(|i| i.contains("_adb-tls") || i.contains(':')),
            })
        })
        .collect()
}

pub fn list_devices(flutter: &Path) -> AppResult<Vec<Device>> {
    let o = proc::run(flutter, &["devices", "--machine"], None, Duration::from_secs(90))?;
    Ok(parse_devices(&o.stdout))
}

pub fn project_dir_exists(p: &Path) -> bool {
    p.join("pubspec.yaml").is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(lines: &[&str]) -> Inner {
        let mut i = Inner::default();
        i.state.status = "starting".into();
        for l in lines {
            i.feed_line(l);
        }
        i
    }

    #[test]
    fn strips_ansi_and_carriage_returns() {
        assert_eq!(strip_ansi("\u{1b}[31mError\u{1b}[0m: x\r"), "Error: x");
    }

    #[test]
    fn real_flutter_compile_error_survives_the_failed_restart_line() {
        let i = feed(&[
            "Performing hot restart...",
            "lib/app.dart:21:6: Error: Expected ';' after this.",
            "this is not dart;",
            "     ^^",
            "lib/app.dart:21:9: Error: 'not' isn't a type.",
            "Performing hot restart...                                          123ms",
            "Try again after fixing the above error(s).",
        ]);
        assert_eq!(i.state.status, "error");
        assert_eq!(i.state.problems[0].file.as_deref(), Some("lib/app.dart"));
        assert_eq!(i.state.problems[0].line, Some(21));
        // Een goede herstart daarna herstelt de status en wist de fouten.
        let mut i = i;
        i.feed_line("Performing hot restart...");
        i.feed_line("Performing hot restart...                                          80ms");
        assert_eq!(i.state.status, "running");
        assert!(i.state.problems.is_empty());
    }

    #[test]
    fn parses_compile_errors() {
        assert_eq!(
            parse_compile_error("lib/main.dart:12:5: Error: Expected ';' after this."),
            Some(("lib/main.dart".into(), 12, "Expected ';' after this.".into()))
        );
        assert_eq!(parse_compile_error("C:\\app\\lib\\a.dart:3:1: Error: Bad"), Some(("C:\\app\\lib\\a.dart".into(), 3, "Bad".into())));
        assert!(parse_compile_error("Error: plain").is_none());
        assert!(parse_compile_error("notes.txt:1:1: Error: x").is_none());
    }

    #[test]
    fn happy_path_reaches_running_and_counts_reloads() {
        let i = feed(&["Launching lib/main.dart on Web Server in debug mode...", "lib/main.dart is being served at http://127.0.0.1:51234"]);
        assert_eq!((i.state.status.as_str(), i.state.url.as_deref()), ("running", Some("http://127.0.0.1:51234")));
        let mut i = i;
        assert!(i.feed_line("Restarted application in 812ms."));
        assert_eq!(i.state.reloads, 1);
        assert!(!i.feed_line("   "), "blank lines change nothing");
    }

    #[test]
    fn real_flutter_3_50_web_server_output() {
        // Letterlijk uit een echte run (zie DECISIONS.md, fase 6).
        let mut i = feed(&[
            "Launching lib/main.dart on Web Server in debug mode...",
            "Waiting for connection from debug service on Web Server...         34.9s",
            "lib/main.dart is being served at http://127.0.0.1:38205",
            "The web-server device requires the Dart Debug Chrome extension for debugging.",
            "Flutter run key commands.",
        ]);
        assert_eq!((i.state.status.as_str(), i.state.reloads), ("running", 0));
        i.state.status = "reloading".into();
        i.feed_line("Performing hot restart...");
        assert_eq!(i.state.reloads, 0, "the line without a time is still busy");
        i.feed_line("Recompile complete. No client connected.");
        assert_eq!(i.state.status, "running", "no browser yet is not a failure");
        i.feed_line("Performing hot restart...                                           71ms");
        assert_eq!((i.state.status.as_str(), i.state.reloads), ("running", 1));
        i.feed_line("Reloaded 1 of 593 libraries in 305ms (compile: 12 ms, reload: 150 ms).");
        assert_eq!(i.state.reloads, 2);
    }

    #[test]
    fn compile_errors_are_collected_with_context_and_cleared_after_a_fix() {
        let mut i = feed(&[
            "lib/main.dart is being served at http://127.0.0.1:1",
            "Performing hot restart...",
            "lib/main.dart:12:5: Error: Expected ';' after this.",
            "  runApp(const MyApp())",
            "                       ^",
            "Failed to compile application.",
        ]);
        assert_eq!(i.state.status, "error");
        assert_eq!(i.state.problems.len(), 1);
        let p = &i.state.problems[0];
        assert_eq!((p.file.as_deref(), p.line), (Some("lib/main.dart"), Some(12)));
        assert!(p.excerpt.contains("runApp(const MyApp())") && p.excerpt.contains('^'));
        i.feed_line("Restarted application in 400ms.");
        assert_eq!((i.state.status.as_str(), i.state.problems.len()), ("running", 0));
    }

    #[test]
    fn runtime_exceptions_become_problems() {
        let i = feed(&[
            "lib/main.dart is being served at http://127.0.0.1:1",
            "══╡ EXCEPTION CAUGHT BY WIDGETS LIBRARY ╞═══════════",
            "The following assertion was thrown building HomeScreen:",
            "setState() called after dispose()",
            "════════════════════════════════════════════════════",
        ]);
        assert_eq!(i.state.problems.len(), 1);
        assert!(i.state.problems[0].message.contains("EXCEPTION CAUGHT"));
        assert!(i.state.problems[0].excerpt.contains("setState() called after dispose()"));
        assert!(!i.state.problems[0].excerpt.contains("════════════════════════════════════════════════════"));
        assert_eq!(i.state.status, "running", "an exception does not stop the app");
    }

    #[test]
    fn log_is_bounded_and_redacted() {
        let mut i = Inner::default();
        for n in 0..(LOG_MAX + 50) {
            i.feed_line(&format!("line {n}"));
        }
        assert_eq!(i.log.len(), LOG_MAX);
        i.feed_line("token=abcdef123456");
        assert!(i.log.last().unwrap().contains("[redacted]"));
    }

    #[test]
    fn devices_only_mobile() {
        let json = r#"Resolving dependencies...
[{"name":"Pixel 8","id":"1A2B3C","isSupported":true,"targetPlatform":"android-arm64","emulator":false,"sdk":"Android 15","capabilities":{}},
 {"name":"sdk gphone","id":"emulator-5554","targetPlatform":"android-x64","emulator":true},
 {"name":"Chrome","id":"chrome","targetPlatform":"web-javascript","emulator":false},
 {"name":"Windows","id":"windows","targetPlatform":"windows-x64","emulator":false},
 {"name":"iPhone","id":"00008110-001","targetPlatform":"ios","emulator":false,"connectionType":"wireless"}]"#;
        let d = parse_devices(json);
        assert_eq!(d.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), ["1A2B3C", "emulator-5554", "00008110-001"]);
        assert!(d[1].emulator && d[2].wireless && !d[0].wireless);
        assert!(parse_devices("garbage").is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn runs_stops_and_reloads_with_a_fake_flutter() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("studio-prev-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("lib")).unwrap();
        std::fs::write(root.join("lib/main.dart"), "a").unwrap();
        let fake = root.join("flutter");
        // Nep-flutter: antwoordt op `run --help`; bij `run` start hij een echte luisteraar via nc-loze bash? Nee: hij print
        // de "served"-regel en reageert op R/q op stdin (zoals flutter run).
        std::fs::write(
            &fake,
            "#!/bin/bash\n\
             if [ \"$2\" = \"--help\" ]; then echo '--web-port --web-hostname'; exit 0; fi\n\
             for a in \"$@\"; do case $a in --web-port=*) port=${a#--web-port=};; esac; done\n\
             echo 'Launching lib/main.dart on Web Server in debug mode...'\n\
             echo \"lib/main.dart is being served at http://127.0.0.1:$port\"\n\
             echo \"$@\" > \"$(dirname \"$0\")/argv.txt\"\n\
             while IFS= read -r -n1 c; do\n\
               case \"$c\" in R) echo 'Restarted application in 10ms.';; q) exit 0;; esac\n\
             done\n",
        )
        .unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();

        let events: Arc<Mutex<Vec<serde_json::Value>>> = Arc::default();
        let sink = events.clone();
        let mgr = PreviewManager::new(Arc::new(move |n, v| {
            if n == "preview-state" {
                sink.lock().unwrap().push(v)
            }
        }));
        mgr.start(&fake, "p1", &root, "web").unwrap();
        let wait = |cond: &dyn Fn(&PreviewState) -> bool| {
            let end = Instant::now() + Duration::from_secs(10);
            while Instant::now() < end {
                if cond(&mgr.state()) {
                    return true;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            false
        };
        assert!(wait(&|s| s.status == "running" && s.url.is_some()), "state: {:?}", mgr.state());
        let argv = (0..100)
            .find_map(|_| {
                std::thread::sleep(Duration::from_millis(50));
                std::fs::read_to_string(root.join("argv.txt")).ok().filter(|t| !t.is_empty())
            })
            .expect("fake flutter wrote its arguments");
        assert!(argv.contains("-d web-server") && argv.contains("--web-hostname=127.0.0.1") && argv.contains("--web-port="));

        // Wijziging in lib/ → automatisch hot restart (het nep-proces antwoordt met "Restarted").
        std::fs::write(root.join("lib/main.dart"), "b").unwrap();
        assert!(wait(&|s| s.reloads >= 1), "no automatic reload: {:?}", mgr.state());
        mgr.reload().unwrap();
        assert!(wait(&|s| s.reloads >= 2));

        mgr.stop();
        assert_eq!(mgr.state().status, "stopped");
        assert!(mgr.reload().is_err());
        assert!(events.lock().unwrap().iter().any(|e| e["status"] == "running"));
        assert!(mgr.log().iter().any(|l| l.contains("is being served")));
        std::fs::remove_dir_all(root).ok();
    }

    #[cfg(unix)]
    #[test]
    fn crash_is_reported_as_a_problem() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("studio-prev-crash-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("lib")).unwrap();
        let fake = root.join("flutter");
        std::fs::write(&fake, "#!/bin/bash\nif [ \"$2\" = \"--help\" ]; then echo '--web-port --web-hostname'; exit 0; fi\necho 'Could not find an option named x' >&2\nexit 64\n").unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mgr = PreviewManager::new(Arc::new(|_, _| {}));
        mgr.start(&fake, "p", &root, "web").unwrap();
        let end = Instant::now() + Duration::from_secs(10);
        while mgr.state().status != "error" && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(50));
        }
        let s = mgr.state();
        assert_eq!(s.status, "error");
        assert!(s.problems[0].message.contains("exit code 64"));
        assert!(s.problems[0].excerpt.contains("Could not find an option"));
    }

    #[test]
    fn rejects_old_flutter_and_weird_device_ids() {
        let root = std::env::temp_dir().join("studio-prev-none");
        let mgr = PreviewManager::new(Arc::new(|_, _| {}));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::create_dir_all(&root).unwrap();
            let fake = root.join("flutter-old");
            std::fs::write(&fake, "#!/bin/sh\necho 'nothing useful'\n").unwrap();
            std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
            assert!(matches!(mgr.start(&fake, "p", &root, "web"), Err(AppError::CommandFailed(m)) if m.contains("outdated")));
            assert!(matches!(mgr.start(&fake, "p", &root, "x; rm -rf /"), Err(AppError::Invalid(_))));
        }
    }

    /// Echte Flutter: `STUDIO_TEST_PROJECT=<map> cargo test real_preview -- --ignored --nocapture`
    /// Start de web-preview, wacht tot hij draait, breekt de code (bouwfout), herstelt, en stopt.
    #[test]
    #[ignore]
    fn real_preview() {
        let dir = std::path::PathBuf::from(std::env::var("STUDIO_TEST_PROJECT").expect("STUDIO_TEST_PROJECT"));
        let flutter = proc::find("flutter").expect("flutter on PATH");
        let mgr = PreviewManager::new(Arc::new(|_, _| {}));
        let mut helper: Option<std::process::Child> = None;
        mgr.start(&flutter, "p", &dir, "web").unwrap();
        let wait = |label: &str, secs: u64, cond: &dyn Fn(&PreviewState) -> bool| {
            let end = Instant::now() + Duration::from_secs(secs);
            while Instant::now() < end {
                if cond(&mgr.state()) {
                    println!("OK  {label}: {:?} url={:?} reloads={} problems={}", mgr.state().status, mgr.state().url, mgr.state().reloads, mgr.state().problems.len());
                    return;
                }
                std::thread::sleep(Duration::from_millis(500));
            }
            println!("TIMEOUT {label}: {:?}\n--- log tail ---\n{}", mgr.state(), mgr.log().iter().rev().take(25).rev().cloned().collect::<Vec<_>>().join("\n"));
            mgr.stop();
            panic!("{label}");
        };
        wait("running", 280, &|s| s.status == "running" && s.url.is_some());
        // Een web-server-preview kan pas herstarten als er een browser is verbonden (in Studio is dat het iframe).
        if let Ok(cmd) = std::env::var("STUDIO_TEST_OPEN_BROWSER") {
            let url = mgr.state().url.unwrap();
            let mut parts = cmd.split_whitespace();
            helper = Some(std::process::Command::new(parts.next().unwrap()).args(parts).arg(&url).stdout(std::process::Stdio::null()).spawn().expect("browser helper"));
            std::thread::sleep(Duration::from_secs(25));
        }
        // 1. gewone wijziging -> automatische hot restart
        let main = dir.join("lib/app.dart");
        let original = std::fs::read_to_string(&main).unwrap();
        std::fs::write(&main, original.replace("debugShowCheckedModeBanner: false,", "debugShowCheckedModeBanner: true,")).unwrap();
        wait("auto reload after an edit", 120, &|s| s.reloads >= 1);
        // 2. bouwfout -> probleem met bestand en regel
        std::fs::write(&main, format!("{original}\nthis is not dart;\n")).unwrap();
        wait("compile error detected", 120, &|s| s.status == "error" && !s.problems.is_empty());
        let p = &mgr.state().problems[0];
        println!("problem: file={:?} line={:?} message={}", p.file, p.line, p.message);
        println!("--- real compile error log ---\n{}\n--- end ---", mgr.log().iter().rev().take(14).rev().cloned().collect::<Vec<_>>().join("\n"));
        // 3. herstel -> weer groen, problemen weg
        std::fs::write(&main, &original).unwrap();
        wait("recovered", 120, &|s| s.status == "running" && s.problems.is_empty());
        mgr.stop();
        if let Some(mut h) = helper {
            let _ = h.kill();
            let _ = h.wait();
        }
        assert_eq!(mgr.state().status, "stopped");
    }
}

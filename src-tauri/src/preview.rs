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

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Failure {
    /// `flutter_missing` | `crashed` | `start_failed`
    pub code: String,
    pub message: String,
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
    /// Waarom de preview niet draait en wat de gebruiker eraan kan doen (bv. Flutter ontbreekt).
    pub failure: Option<Failure>,
    /// `iframe` (web-server, geen inspector) of `inspect` (Chrome van `flutter run`, het scherm komt als beeldstroom).
    pub mode: String,
}

#[derive(Default)]
pub struct Inner {
    pub state: PreviewState,
    pub log: Vec<String>,
    /// VM-service van de draaiende app (alleen in `inspect`-modus).
    pub vm_ws: Option<String>,
    pub web_port: u16,
    pub cdp_port: u16,
    pub viewport: (u32, u32),
    /// Het `flutter run`-proces eindigde onverwacht (de supervisor start opnieuw, maximaal drie keer).
    pub crashed: bool,
    /// De route waar de app nu is (alleen in `inspect`-modus bekend).
    pub route: String,
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

        if line.contains("Debug service listening on ") {
            if let Some(i) = line.find("ws://") {
                self.vm_ws = Some(line[i..].trim().to_string());
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
    /// Stuurt een nieuw formaat (in CSS-pixels) naar de beeldstroom.
    viewport_tx: Option<std::sync::mpsc::Sender<(u32, u32)>>,
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
    /// Wordt bij elke start door de gebruiker/de app en bij elke stop opgehoogd; de supervisor stopt als zijn nummer niet meer klopt.
    generation: Arc<std::sync::atomic::AtomicU64>,
}

/// Hoe vaak de preview na een crash opnieuw wordt gestart.
pub const MAX_RESTARTS: u32 = 3;

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
        Self { inner: Arc::new(Mutex::new(Inner { state: PreviewState { status: "stopped".into(), target: "web".into(), ..Default::default() }, ..Default::default() })), run: Mutex::new(None), emit, generation: Arc::new(std::sync::atomic::AtomicU64::new(0)) }
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

    /// Stopt de preview (en de supervisor): bij het verlaten van het project of bij een nieuwe start.
    pub fn stop(&self) {
        self.generation.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.stop_process();
    }

    /// Meldt dat de preview niet kan draaien, met een code waar de UI een vriendelijke melding en oplossing bij kan tonen.
    pub fn set_failure(&self, project_id: &str, code: &str, message: &str) {
        {
            let mut i = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            i.state = PreviewState { project_id: Some(project_id.into()), status: "error".into(), target: "web".into(), mode: "iframe".into(), failure: Some(Failure { code: code.into(), message: message.into() }), ..Default::default() };
        }
        self.publish();
    }

    fn stop_process(&self) {
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

    /// Start `flutter run` en houdt het in de gaten: eindigt het proces onverwacht, dan start Studio het opnieuw (maximaal drie keer).
    /// Compilatiefouten tellen niet: dan blijft het proces draaien en wacht het op de volgende wijziging.
    pub fn start_supervised(self: &Arc<Self>, flutter: &Path, project_id: &str, dir: &Path, target: &str) -> AppResult<()> {
        use std::sync::atomic::Ordering::SeqCst;
        let gen = self.generation.fetch_add(1, SeqCst) + 1;
        if let Err(e) = self.start_inner(flutter, project_id, dir, target) {
            self.set_failure(project_id, if matches!(e, AppError::ToolMissing(_)) { "flutter_missing" } else { "start_failed" }, &crate::logging::redact(&e.to_string()));
            return Err(e);
        }
        let (me, flutter, project_id, dir, target) = (self.clone(), flutter.to_path_buf(), project_id.to_string(), dir.to_path_buf(), target.to_string());
        std::thread::spawn(move || {
            let mut restarts = 0u32;
            let mut running_since: Option<Instant> = None;
            loop {
                std::thread::sleep(Duration::from_millis(500));
                if me.generation.load(SeqCst) != gen {
                    return;
                }
                let (crashed, status) = {
                    let i = me.inner.lock().unwrap_or_else(|e| e.into_inner());
                    (i.crashed, i.state.status.clone())
                };
                // Een proces dat een minuut goed draait telt als hersteld.
                if status == "running" {
                    if running_since.get_or_insert_with(Instant::now).elapsed() > Duration::from_secs(60) {
                        restarts = 0;
                    }
                } else {
                    running_since = None;
                }
                if !crashed {
                    continue;
                }
                if restarts >= MAX_RESTARTS {
                    me.inner.lock().unwrap_or_else(|e| e.into_inner()).crashed = false;
                    {
                        let mut i = me.inner.lock().unwrap_or_else(|e| e.into_inner());
                        i.state.failure = Some(Failure { code: "crashed".into(), message: "The preview stopped several times in a row.".into() });
                    }
                    me.publish();
                    return;
                }
                restarts += 1;
                log::warn!("preview crashed; restart {restarts} of {MAX_RESTARTS}");
                if me.start_inner(&flutter, &project_id, &dir, &target).is_err() {
                    me.inner.lock().unwrap_or_else(|e| e.into_inner()).crashed = true;
                }
            }
        });
        Ok(())
    }

    /// Start `flutter run` voor een project (zonder supervisor; voor tests). `target` = "web" of een apparaat-id uit `flutter devices`.
    pub fn start(&self, flutter: &Path, project_id: &str, dir: &Path, target: &str) -> AppResult<()> {
        self.generation.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.start_inner(flutter, project_id, dir, target)
    }

    fn start_inner(&self, flutter: &Path, project_id: &str, dir: &Path, target: &str) -> AppResult<()> {
        self.stop_process();
        let web = target == "web";
        let help = proc::run(flutter, &["run", "--help", "-v"], Some(dir), Duration::from_secs(120))?.combined();
        // Preview-modus: de app slaat inlogbeveiliging over en gebruikt voorbeelddata (`lib/core/preview_mode.dart`); nooit in release-builds.
        let mut args: Vec<String> = vec!["run".into(), "--dart-define=STUDIO_PREVIEW=true".into()];
        let mut url = None;
        let mut inspect: Option<(std::path::PathBuf, u16, u16)> = None;
        if web {
            for f in ["--web-port", "--web-hostname"] {
                if !help.contains(f) {
                    return Err(AppError::CommandFailed(format!("outdated flutter: `flutter run` has no {f}")));
                }
            }
            let port = free_port()?;
            // Met een browser op de computer draaien we de app in die (headless) browser: alleen daar heeft Flutter een VM-service,
            // en dus de widget inspector. Zonder browser (of met STUDIO_PREVIEW_IFRAME=1) de oude web-server-variant in een iframe.
            let browser = crate::screenshot::find_browser().filter(|_| std::env::var("STUDIO_PREVIEW_IFRAME").is_err() && help.contains("--web-browser-debug-port"));
            if let Some(b) = browser {
                let dbg = free_port()?;
                args.extend(["-d".into(), "chrome".into(), "--web-hostname=127.0.0.1".into(), format!("--web-port={port}"), format!("--web-browser-debug-port={dbg}"), "--web-browser-flag=--headless=new".into()]);
                if std::env::var("STUDIO_BROWSER_NO_SANDBOX").is_ok() {
                    args.push("--web-browser-flag=--no-sandbox".into());
                }
                if std::env::var("STUDIO_OFFLINE_WEB").is_ok() {
                    args.push("--no-web-resources-cdn".into());
                }
                inspect = Some((b, port, dbg));
            } else {
                args.extend(["-d".into(), "web-server".into(), "--web-hostname=127.0.0.1".into(), format!("--web-port={port}")]);
            }
            url = Some(format!("http://127.0.0.1:{port}"));
        } else {
            if !target.chars().all(|c| c.is_ascii_alphanumeric() || "-_.:".contains(c)) {
                return Err(AppError::Invalid("invalid device id".into()));
            }
            args.extend(["-d".into(), target.to_string()]);
        }

        {
            let mut i = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            *i = Inner { state: PreviewState { project_id: Some(project_id.into()), status: "starting".into(), target: target.into(), url: None, problems: vec![], reloads: 0, failure: None, mode: if inspect.is_some() { "inspect".into() } else { "iframe".into() } }, web_port: inspect.as_ref().map(|i| i.1).unwrap_or(0), cdp_port: inspect.as_ref().map(|i| i.2).unwrap_or(0), viewport: (390, 844), ..Default::default() };
        }
        self.publish();

        let mut cmd = proc::command(flutter);
        cmd.args(&args).current_dir(dir).stdin(Stdio::piped());
        if let Some((b, _, _)) = &inspect {
            cmd.env("CHROME_EXECUTABLE", b);
        }
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
                    i.crashed = code != Some(0);
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
        let viewport_tx = inspect.as_ref().map(|(_, web_port, cdp_port)| spawn_screencast(*cdp_port, *web_port, self.emit.clone(), stopping.clone()));
        if let Some((_, web_port, cdp_port)) = inspect.as_ref() {
            spawn_route_poll(*cdp_port, *web_port, self.inner.clone(), self.emit.clone(), stopping.clone());
        }
        *self.run.lock().unwrap_or_else(|e| e.into_inner()) = Some(Running { viewport_tx, child, stdin, stopping, _watcher: watcher, web });
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

    /// Nieuw formaat voor het scherm in de preview (CSS-pixels). Geldt ook voor het omzetten van een vak naar widgets.
    pub fn set_viewport(&self, w: u32, h: u32) {
        let (w, h) = (w.clamp(200, 2000), h.clamp(300, 3000));
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).viewport = (w, h);
        if let Some(tx) = self.run.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|r| r.viewport_tx.as_ref()) {
            let _ = tx.send((w, h));
        }
    }

    /// Laat de preview naar een route gaan (pagina aangeklikt in de lijst). Alleen in `inspect`-modus; anders `Unsupported`.
    pub fn navigate(&self, route: &str) -> AppResult<()> {
        let (web_port, cdp_port, mode) = {
            let i = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            (i.web_port, i.cdp_port, i.state.mode.clone())
        };
        if mode != "inspect" {
            return Err(AppError::Unsupported("preview navigation".into()));
        }
        crate::inspect::navigate_to(&crate::inspect::page_target(cdp_port, web_port)?, route)
    }

    /// Zet een vak (procenten van het telefoonscherm) om in widgets uit de code van het project, via de widget inspector.
    pub fn select_box(&self, project_dir: &Path, bbox: [f64; 4], screens: &[(String, String)]) -> AppResult<crate::selection::Selection> {
        let (vm, web_port, cdp_port, viewport, mode) = {
            let i = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            (i.vm_ws.clone(), i.web_port, i.cdp_port, i.viewport, i.state.mode.clone())
        };
        let vm = vm.filter(|_| mode == "inspect").ok_or_else(|| AppError::NotFound("inspector".into()))?;
        let page = crate::inspect::page_target(cdp_port, web_port)?;
        crate::inspect::select_box(&vm, &page, project_dir, viewport, bbox, screens)
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

/// Draad die het scherm van de app (de pagina in Chrome) als JPEG-beelden doorstuurt. Stuurt hooguit ~8 beelden per seconde
/// naar de UI, maar bevestigt elk beeld meteen (anders stopt Chrome met sturen).
fn spawn_screencast(cdp_port: u16, web_port: u16, emit: EmitFn, stopping: Arc<std::sync::atomic::AtomicBool>) -> std::sync::mpsc::Sender<(u32, u32)> {
    use crate::inspect::{page_target, viewport_params, Rpc};
    use serde_json::json;
    let (tx, rx) = channel::<(u32, u32)>();
    std::thread::spawn(move || {
        let stop = || stopping.load(std::sync::atomic::Ordering::SeqCst);
        let deadline = Instant::now() + Duration::from_secs(180);
        let url = loop {
            if stop() || Instant::now() > deadline {
                return;
            }
            match page_target(cdp_port, web_port) {
                Ok(u) => break u,
                Err(_) => std::thread::sleep(Duration::from_millis(500)),
            }
        };
        let Ok(mut rpc) = Rpc::connect(&url, Duration::from_millis(200)) else { return };
        let start = |rpc: &mut Rpc| rpc.call("Page.startScreencast", json!({ "format": "jpeg", "quality": 70, "everyNthFrame": 1 })).is_ok();
        if rpc.call("Page.enable", json!({})).is_err() || !start(&mut rpc) {
            return;
        }
        let mut last = Instant::now() - Duration::from_secs(1);
        while !stop() {
            let mut resized = None;
            while let Ok(v) = rx.try_recv() {
                resized = Some(v);
            }
            if let Some((w, h)) = resized {
                let _ = rpc.call("Emulation.setDeviceMetricsOverride", viewport_params(w, h));
                let _ = rpc.call("Page.stopScreencast", json!({}));
                let _ = start(&mut rpc);
            }
            match rpc.read() {
                Ok(Some(m)) if m["method"] == "Page.screencastFrame" => {
                    let p = &m["params"];
                    if let Some(sid) = p["sessionId"].as_u64() {
                        let _ = rpc.send("Page.screencastFrameAck", json!({ "sessionId": sid }));
                    }
                    if last.elapsed() >= Duration::from_millis(120) {
                        last = Instant::now();
                        emit("preview-frame", json!({ "data": p["data"], "width": p["metadata"]["deviceWidth"], "height": p["metadata"]["deviceHeight"] }));
                    }
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
        rpc.close();
    });
    tx
}

/// Draad die ongeveer elke seconde kijkt op welke route de app is en dat meldt als het verandert (ook als de gebruiker in de preview zelf
/// naar een andere pagina gaat of na een herstart), zodat de lijst met pagina's gelijk blijft aan het scherm.
fn spawn_route_poll(cdp_port: u16, web_port: u16, inner: Arc<Mutex<Inner>>, emit: EmitFn, stopping: Arc<std::sync::atomic::AtomicBool>) {
    use crate::inspect::{current_route, page_target, Rpc};
    std::thread::spawn(move || {
        let stop = || stopping.load(std::sync::atomic::Ordering::SeqCst);
        let mut rpc: Option<Rpc> = None;
        let mut last = String::new();
        while !stop() {
            std::thread::sleep(Duration::from_millis(1000));
            if rpc.is_none() {
                rpc = page_target(cdp_port, web_port).ok().and_then(|u| Rpc::connect(&u, Duration::from_millis(500)).ok());
            }
            let Some(r) = rpc.as_mut() else { continue };
            match current_route(r) {
                Ok(route) if route != last => {
                    last = route.clone();
                    inner.lock().unwrap_or_else(|e| e.into_inner()).route = route.clone();
                    emit("preview-route", serde_json::json!({ "route": route }));
                }
                Ok(_) => {}
                Err(_) => rpc = None,
            }
        }
    });
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
    fn the_vm_service_address_is_taken_from_the_chrome_device_output() {
        let i = feed(&["Debug service listening on ws://127.0.0.1:35221/YuHntcXrtpI=/ws", "A Dart VM Service on Chrome is available at: http://127.0.0.1:35221/YuHntcXrtpI="]);
        assert_eq!(i.vm_ws.as_deref(), Some("ws://127.0.0.1:35221/YuHntcXrtpI=/ws"));
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
    fn fake_flutter_crashing(root: &std::path::Path, crashes: u32) -> std::path::PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let fake = root.join("flutter");
        std::fs::write(
            &fake,
            format!(
                "#!/bin/bash\n\
                 if [ \"$2\" = \"--help\" ]; then echo '--web-port --web-hostname'; exit 0; fi\n\
                 d=$(dirname \"$0\"); echo x >> \"$d/starts.txt\"; n=$(wc -l < \"$d/starts.txt\")\n\
                 for a in \"$@\"; do case $a in --web-port=*) port=${{a#--web-port=}};; esac; done\n\
                 echo 'Launching lib/main.dart on Web Server in debug mode...'\n\
                 if [ \"$n\" -le {crashes} ]; then exit 1; fi\n\
                 echo \"lib/main.dart is being served at http://127.0.0.1:$port\"\n\
                 while IFS= read -r -n1 c; do case \"$c\" in q) exit 0;; esac; done\n"
            ),
        )
        .unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        fake
    }

    /// Een crash wordt hersteld (hoogstens drie keer); houdt het niet op, dan staat er een melding en geeft Studio het op.
    #[cfg(unix)]
    #[test]
    fn a_crashing_preview_is_restarted_up_to_three_times() {
        let root = std::env::temp_dir().join(format!("studio-prev-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("lib")).unwrap();
        // 1. crasht twee keer, de derde start slaagt
        let fake = fake_flutter_crashing(&root, 2);
        let mgr = Arc::new(PreviewManager::new(Arc::new(|_, _| {})));
        mgr.start_supervised(&fake, "p1", &root, "web").unwrap();
        let end = Instant::now() + Duration::from_secs(30);
        while Instant::now() < end && mgr.state().status != "running" {
            std::thread::sleep(Duration::from_millis(100));
        }
        assert_eq!(mgr.state().status, "running", "{:?}", mgr.state());
        assert_eq!(std::fs::read_to_string(root.join("starts.txt")).unwrap().lines().count(), 3);
        mgr.stop();
        // 2. crasht altijd: één start en drie herstarts, dan een melding
        std::fs::remove_file(root.join("starts.txt")).unwrap();
        let fake = fake_flutter_crashing(&root, 99);
        let mgr = Arc::new(PreviewManager::new(Arc::new(|_, _| {})));
        mgr.start_supervised(&fake, "p1", &root, "web").unwrap();
        let end = Instant::now() + Duration::from_secs(40);
        while Instant::now() < end && mgr.state().failure.is_none() {
            std::thread::sleep(Duration::from_millis(100));
        }
        let st = mgr.state();
        assert_eq!(st.failure.as_ref().map(|f| f.code.as_str()), Some("crashed"), "{st:?}");
        assert_eq!(std::fs::read_to_string(root.join("starts.txt")).unwrap().lines().count(), 4, "one start and three restarts, never more");
        mgr.stop();
        // 3. zonder Flutter: een melding met een code, nooit een startknop
        let none = Arc::new(PreviewManager::new(Arc::new(|_, _| {})));
        none.set_failure("p1", "flutter_missing", "Flutter is not installed");
        assert_eq!(none.state().failure.unwrap().code, "flutter_missing");
        std::fs::remove_dir_all(root).ok();
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
        assert!(argv.contains("--dart-define=STUDIO_PREVIEW=true"), "the preview always runs in preview mode: {argv}");

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
    /// Echte keten: preview in Chrome (inspect-modus), beeldstroom, vak → widgets via de inspector, en na een regelverschuiving
    /// (hot restart) opnieuw. Vereist Flutter, Chrome en een testproject zoals `experiments/inspector/probe`.
    #[test]
    #[ignore]
    fn real_inspect_flow() {
        let dir = std::path::PathBuf::from(std::env::var("STUDIO_TEST_PROJECT").expect("STUDIO_TEST_PROJECT"));
        let flutter = proc::find("flutter").expect("flutter on PATH");
        let frames = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let f2 = frames.clone();
        let mgr = PreviewManager::new(Arc::new(move |n, _| {
            if n == "preview-frame" {
                f2.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
        }));
        mgr.start(&flutter, "p", &dir, "web").unwrap();
        assert_eq!(mgr.state().mode, "inspect");
        let end = Instant::now() + Duration::from_secs(280);
        while Instant::now() < end && !(mgr.state().status == "running" && frames.load(std::sync::atomic::Ordering::SeqCst) > 0) {
            std::thread::sleep(Duration::from_millis(500));
        }
        // De VM-service meldt zich pas als de app in de browser geladen is.
        let end = Instant::now() + Duration::from_secs(120);
        while Instant::now() < end && mgr.inner.lock().unwrap().vm_ws.is_none() {
            std::thread::sleep(Duration::from_millis(500));
        }
        assert!(mgr.inner.lock().unwrap().vm_ws.is_some(), "VM service address\n{}", mgr.log().iter().rev().take(15).rev().cloned().collect::<Vec<_>>().join("\n"));
        println!("running, frames so far: {}", frames.load(std::sync::atomic::Ordering::SeqCst));
        assert!(frames.load(std::sync::atomic::Ordering::SeqCst) > 0, "the screencast delivers frames");
        mgr.set_viewport(500, 644);
        std::thread::sleep(Duration::from_secs(3));
        let screens = vec![("HomeScreen".to_string(), "lib/home_screen.dart".to_string())];

        // 1. de knop "Open list" (gedeeld component PrimaryButton) wordt de aanroep op het scherm
        let sel = mgr.select_box(&dir, [3.0, 25.4, 22.0, 5.0], &screens).unwrap();
        println!("selection 1: {}", sel.prompt());
        let t = sel.targets.iter().find(|t| t.widget == "PrimaryButton").expect("PrimaryButton");
        assert_eq!((t.file.as_str(), t.start_line, t.end_line, t.via_shared), ("lib/home_screen.dart", 22, 25, true));
        assert_eq!(t.text.as_deref(), Some("Open list"));
        assert_eq!(sel.screen.as_deref(), Some("HomeScreen"));

        // 2. een vak om twee teksten: beide, zonder de omvattende Column
        let sel = mgr.select_box(&dir, [3.0, 10.5, 40.0, 12.0], &screens).unwrap();
        println!("selection 2: {}", sel.prompt());
        let texts: Vec<_> = sel.targets.iter().map(|t| (t.widget.as_str(), t.start_line)).collect();
        assert!(texts.contains(&("Text", 18)) && texts.contains(&("Text", 20)), "{texts:?}");
        assert!(!texts.iter().any(|(w, _)| *w == "Column"), "the surrounding Column is dropped: {texts:?}");

        // 3. regels verschuiven + hot restart: de feiten zijn weer actueel
        let file = dir.join("lib/home_screen.dart");
        let original = std::fs::read_to_string(&file).unwrap();
        std::fs::write(&file, format!("// extra 1\n// extra 2\n{original}")).unwrap();
        let before = mgr.state().reloads;
        let end = Instant::now() + Duration::from_secs(90);
        while Instant::now() < end && mgr.state().reloads <= before {
            std::thread::sleep(Duration::from_millis(500));
        }
        std::thread::sleep(Duration::from_secs(2));
        let sel = mgr.select_box(&dir, [3.0, 25.4, 22.0, 5.0], &screens);
        std::fs::write(&file, &original).unwrap();
        let sel = sel.unwrap();
        let t = sel.targets.iter().find(|t| t.widget == "PrimaryButton").unwrap();
        println!("after shift: {} lines {}-{}", t.file, t.start_line, t.end_line);
        assert_eq!((t.start_line, t.end_line), (24, 27));
        mgr.stop();
    }

    /// Echte keten met een go_router-project (`STUDIO_TEST_PROJECT`, bv. met een inlog-omleiding): navigeren naar een pagina zonder herladen,
    /// de route teruglezen (ook voor een pagina met parameter) en in preview-modus de inlogbeveiliging overslaan.
    #[test]
    #[ignore]
    fn real_preview_navigation_and_preview_mode() {
        let dir = std::path::PathBuf::from(std::env::var("STUDIO_TEST_PROJECT").expect("STUDIO_TEST_PROJECT"));
        let flutter = proc::find("flutter").expect("flutter on PATH");
        let routes: Arc<Mutex<Vec<String>>> = Arc::default();
        let sink = routes.clone();
        let mgr = PreviewManager::new(Arc::new(move |n, v| {
            if n == "preview-route" {
                sink.lock().unwrap().push(v["route"].as_str().unwrap_or("").to_string());
            }
        }));
        mgr.start(&flutter, "p", &dir, "web").unwrap();
        let end = Instant::now() + Duration::from_secs(280);
        while Instant::now() < end && !(mgr.state().status == "running" && !routes.lock().unwrap().is_empty()) {
            std::thread::sleep(Duration::from_millis(500));
        }
        assert_eq!(routes.lock().unwrap().first().map(String::as_str), Some("/"), "the app starts on the start page");
        let wait_route = |r: &str| {
            let end = Instant::now() + Duration::from_secs(15);
            while Instant::now() < end {
                if routes.lock().unwrap().last().map(String::as_str) == Some(r) {
                    return true;
                }
                std::thread::sleep(Duration::from_millis(200));
            }
            false
        };
        for r in ["/settings", "/item/7", "/account", "/"] {
            mgr.navigate(r).unwrap();
            assert!(wait_route(r), "route {r} not reached; saw {:?}", routes.lock().unwrap());
        }
        // `/account` heeft inlog nodig (omleiding naar /login), maar in preview-modus blijft het `/account`
        assert!(routes.lock().unwrap().iter().any(|r| r == "/account") && !routes.lock().unwrap().iter().any(|r| r == "/login"), "{:?}", routes.lock().unwrap());
        assert!(mgr.navigate("/a'b").is_err() && mgr.navigate("settings").is_err(), "unsafe routes are refused");
        mgr.stop();
    }

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

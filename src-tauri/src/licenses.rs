//! Android-licenties accepteren zonder terminal: `sdkmanager --licenses` (of `flutter doctor --android-licenses`) als gewoon
//! subproces met pipes, de vragen worden herkend in de uitvoer en met `y` beantwoord, ná een uitdrukkelijke toestemming van de gebruiker.
//! Het subproces krijgt dezelfde omgeving als een nieuwe cmd van de gebruiker (op Windows uit het register, niet alleen wat Studio
//! bij het starten kreeg), met Java en `sdkmanager` expliciet opgezocht.

use serde::Serialize;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

pub type Env = HashMap<String, String>;

// ───────────── omgeving ─────────────

/// Haalt de waarde van `name` uit de uitvoer van `reg query <sleutel> /v <name>` (`    Path    REG_EXPAND_SZ    C:\x;%USERPROFILE%\y`).
pub fn parse_reg_value(out: &str, name: &str) -> Option<String> {
    for line in out.lines() {
        let t = line.trim();
        if t.len() > name.len() && t[..name.len()].eq_ignore_ascii_case(name) && t[name.len()..].starts_with(char::is_whitespace) {
            let rest = t[name.len()..].trim_start();
            for kind in ["REG_EXPAND_SZ", "REG_SZ"] {
                if let Some(v) = rest.strip_prefix(kind) {
                    return Some(v.trim().to_string());
                }
            }
        }
    }
    None
}

/// Vervangt `%NAAM%` door de waarde uit `lookup` (hoofdletterongevoelig); onbekende namen blijven staan.
pub fn expand_vars(s: &str, lookup: &dyn Fn(&str) -> Option<String>) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find('%') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        match after.find('%') {
            Some(j) if j > 0 && !after[..j].contains(char::is_whitespace) => match lookup(&after[..j]) {
                Some(v) => {
                    out.push_str(&v);
                    rest = &after[j + 1..];
                }
                None => {
                    out.push('%');
                    rest = after;
                }
            },
            _ => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Voegt twee PATH-lijsten samen zonder dubbelen (volgorde: eerst `a`, dan `b`).
pub fn merge_paths(a: &str, b: &str, sep: char) -> String {
    let mut seen: Vec<String> = vec![];
    let mut out: Vec<&str> = vec![];
    for p in a.split(sep).chain(b.split(sep)).filter(|p| !p.trim().is_empty()) {
        let key = p.trim_end_matches(['\\', '/']).to_lowercase();
        if !seen.contains(&key) {
            seen.push(key);
            out.push(p);
        }
    }
    out.join(&sep.to_string())
}

/// Samenstellen van de omgeving zoals een nieuwe cmd die zou krijgen: machine- en gebruikersvariabelen over de huidige omgeving heen.
pub fn combine_env(process: &Env, machine: &Env, user: &Env) -> Env {
    let mut env: Env = process.iter().map(|(k, v)| (k.to_uppercase(), v.clone())).collect();
    let mut fresh: Env = HashMap::new();
    for (k, v) in machine.iter().chain(user.iter()) {
        let k = k.to_uppercase();
        if k == "PATH" {
            let cur = fresh.get("PATH").cloned().unwrap_or_default();
            fresh.insert(k, merge_paths(&cur, v, ';'));
        } else {
            fresh.insert(k, v.clone());
        }
    }
    // %VARIABELEN% in de waarden uitbreiden met wat er al is
    let snapshot = {
        let mut s = env.clone();
        s.extend(fresh.clone());
        s
    };
    let lookup = |n: &str| snapshot.get(&n.to_uppercase()).cloned();
    for v in fresh.values_mut() {
        *v = expand_vars(v, &lookup);
    }
    for (k, v) in fresh {
        if k == "PATH" {
            // wat Studio zelf in zijn PATH heeft (bv. via een eerdere start) blijft ook beschikbaar
            let cur = env.get("PATH").cloned().unwrap_or_default();
            env.insert(k, merge_paths(&v, &cur, ';'));
        } else {
            env.insert(k, v);
        }
    }
    env
}

#[cfg(windows)]
fn read_registry(key: &str) -> Env {
    let mut out = Env::new();
    // één `reg query` per sleutel geeft alle waarden
    if let Ok(o) = Command::new("reg").args(["query", key]).stdin(Stdio::null()).output() {
        let text = String::from_utf8_lossy(&o.stdout).into_owned();
        for line in text.lines() {
            let t = line.trim();
            for kind in ["REG_EXPAND_SZ", "REG_SZ"] {
                if let Some(i) = t.find(kind) {
                    let name = t[..i].trim();
                    let value = t[i + kind.len()..].trim();
                    if !name.is_empty() {
                        out.insert(name.to_string(), value.to_string());
                    }
                }
            }
        }
    }
    out
}

/// De actuele omgeving van het systeem. Op Windows: machine + gebruiker uit het register over Studio's eigen omgeving heen,
/// zodat PATH-wijzigingen na een installatie zichtbaar zijn zonder Studio te herstarten.
pub fn system_env() -> Env {
    let process: Env = std::env::vars().collect();
    #[cfg(windows)]
    {
        let machine = read_registry(r"HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\Environment");
        let user = read_registry(r"HKCU\Environment");
        return combine_env(&process, &machine, &user);
    }
    #[cfg(not(windows))]
    {
        process.into_iter().map(|(k, v)| (k.to_uppercase(), v)).collect()
    }
}

/// Zet de actuele systeemomgeving ook in Studio's eigen proces (alleen Windows), zodat controles als `flutter` en `git` zien wat een
/// installatie net aan PATH heeft toegevoegd, zonder dat Studio opnieuw gestart hoeft te worden.
pub fn refresh_process_env() {
    #[cfg(windows)]
    {
        for (k, v) in system_env() {
            if ["PATH", "JAVA_HOME", "ANDROID_HOME", "ANDROID_SDK_ROOT", "FLUTTER_ROOT", "PUB_CACHE"].contains(&k.as_str()) {
                std::env::set_var(&k, &v);
            }
        }
    }
}

// ───────────── Java, SDK en sdkmanager opzoeken ─────────────

fn exe(name: &str) -> String {
    if cfg!(windows) { format!("{name}.exe") } else { name.to_string() }
}

fn java_in(home: &Path) -> Option<PathBuf> {
    let p = home.join("bin").join(exe("java"));
    p.is_file().then_some(p)
}

/// Eerste submap van `dir` (alfabetisch laatste = nieuwste) die een Java bevat.
fn newest_java_under(dir: &Path) -> Option<PathBuf> {
    let mut subs: Vec<PathBuf> = std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    subs.sort();
    subs.into_iter().rev().find(|p| java_in(p).is_some())
}

/// Zoekt een Java-installatie (de map met `bin/java`): JAVA_HOME, PATH, Android Studio's meegeleverde JBR, bekende installatiemappen.
pub fn find_java_home(env: &Env) -> Option<PathBuf> {
    if let Some(h) = env.get("JAVA_HOME").map(PathBuf::from) {
        if java_in(&h).is_some() {
            return Some(h);
        }
    }
    let sep = if cfg!(windows) { ';' } else { ':' };
    if let Some(path) = env.get("PATH") {
        for d in path.split(sep).filter(|d| !d.is_empty()) {
            let d = PathBuf::from(d);
            if d.join(exe("java")).is_file() {
                if let Some(home) = d.parent() {
                    return Some(home.to_path_buf());
                }
            }
        }
    }
    let var = |n: &str| env.get(n).map(PathBuf::from);
    let mut fixed: Vec<PathBuf> = vec![];
    let mut roots: Vec<PathBuf> = vec![];
    for pf in ["PROGRAMFILES", "PROGRAMW6432", "PROGRAMFILES(X86)"] {
        if let Some(p) = var(pf) {
            fixed.push(p.join("Android/Android Studio/jbr"));
            for sub in ["Eclipse Adoptium", "Java", "Microsoft", "Zulu", "Amazon Corretto", "BellSoft"] {
                roots.push(p.join(sub));
            }
        }
    }
    if let Some(l) = var("LOCALAPPDATA") {
        fixed.push(l.join("Programs/Android Studio/jbr"));
    }
    if let Some(h) = var("HOME").or_else(|| var("USERPROFILE")) {
        fixed.push(h.join("Applications/Android Studio.app/Contents/jbr/Contents/Home"));
    }
    fixed.push(PathBuf::from("/Applications/Android Studio.app/Contents/jbr/Contents/Home"));
    fixed.push(PathBuf::from("/opt/android-studio/jbr"));
    roots.push(PathBuf::from("/usr/lib/jvm"));
    fixed.into_iter().find(|p| java_in(p).is_some()).or_else(|| roots.iter().find_map(|r| newest_java_under(r)))
}

/// Zoekt de Android SDK: ANDROID_HOME, ANDROID_SDK_ROOT, standaardlocaties van Android Studio.
pub fn find_sdk_root(env: &Env) -> Option<PathBuf> {
    let mut c: Vec<PathBuf> = vec![];
    for n in ["ANDROID_HOME", "ANDROID_SDK_ROOT"] {
        if let Some(v) = env.get(n) {
            c.push(PathBuf::from(v));
        }
    }
    if let Some(l) = env.get("LOCALAPPDATA") {
        c.push(PathBuf::from(l).join("Android/Sdk"));
    }
    if let Some(h) = env.get("HOME").or_else(|| env.get("USERPROFILE")) {
        let h = PathBuf::from(h);
        c.push(h.join("Android/Sdk"));
        c.push(h.join("Library/Android/sdk"));
    }
    c.into_iter().find(|p| p.is_dir())
}

/// `cmdline-tools/<versie>/bin/sdkmanager`: eerst `latest`, dan de nieuwste andere map; oude `tools/bin` als laatste redmiddel.
pub fn find_sdkmanager(sdk: &Path) -> Option<PathBuf> {
    let name = if cfg!(windows) { "sdkmanager.bat" } else { "sdkmanager" };
    let latest = sdk.join("cmdline-tools/latest/bin").join(name);
    if latest.is_file() {
        return Some(latest);
    }
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(sdk.join("cmdline-tools")).ok().into_iter().flat_map(|rd| rd.flatten().map(|e| e.path())).collect();
    dirs.sort();
    for d in dirs.into_iter().rev() {
        let p = d.join("bin").join(name);
        if p.is_file() {
            return Some(p);
        }
    }
    let old = sdk.join("tools/bin").join(name);
    old.is_file().then_some(old)
}

// ───────────── de vragen herkennen ─────────────

/// Houdt bij wat `sdkmanager` tot nu toe schreef en wat daaruit volgt.
#[derive(Debug, Default)]
pub struct Tracker {
    buf: String,
    answered_upto: usize,
    /// Aantal nog niet geaccepteerde licenties (uit "7 of 7 SDK package licenses not accepted").
    pub total: Option<u32>,
    pub accepted: u32,
    /// "All SDK package licenses accepted" gezien.
    pub done: bool,
    /// Er zijn geen vragen gesteld omdat alles al geaccepteerd was.
    pub already: bool,
}

#[derive(Debug, PartialEq)]
pub enum Step {
    /// Er staat een vraag klaar: antwoord `y`.
    Answer,
    Progress,
}

/// Staat de uitvoer (zonder afsluitende spaties) op een ja/nee-vraag? Herkent `(y/N)`, `(y/n)`, `[y/N]`, met of zonder `?`/`:`.
pub fn ends_with_prompt(text: &str) -> bool {
    let t = text.trim_end();
    let t = t.trim_end_matches([':', '?']).trim_end();
    let l = t.to_lowercase();
    l.ends_with("(y/n)") || l.ends_with("[y/n]")
}

impl Tracker {
    pub fn feed(&mut self, chunk: &str) -> Vec<Step> {
        self.buf.push_str(chunk);
        let mut steps = vec![];
        if self.total.is_none() {
            self.total = parse_total(&self.buf);
            if self.total.is_some() {
                steps.push(Step::Progress);
            }
        }
        if !self.done && self.buf.contains("licenses accepted") {
            self.done = true;
            self.already = self.accepted == 0 && self.total.is_none();
            steps.push(Step::Progress);
        }
        if self.buf.len() > self.answered_upto && ends_with_prompt(&self.buf) {
            // de eerste vraag ("Review licenses that have not been accepted?") telt niet als een licentie
            let review = self.buf[self.answered_upto..].to_lowercase().contains("review licenses");
            self.answered_upto = self.buf.len();
            if !review {
                self.accepted += 1;
            }
            steps.push(Step::Answer);
            steps.push(Step::Progress);
        }
        steps
    }
    pub fn raw(&self) -> &str {
        &self.buf
    }
}

/// `7 of 7 SDK package licenses not accepted` → 7
pub fn parse_total(text: &str) -> Option<u32> {
    let i = text.find("SDK package license")?;
    let before = text[..i].trim_end();
    let before = before.strip_suffix("not accepted").unwrap_or(before).trim_end();
    let words: Vec<&str> = before.split_whitespace().collect();
    // [..., "3", "of", "6"]: het eerste getal is het aantal dat nog geaccepteerd moet worden
    (words.len() >= 3 && words[words.len() - 2] == "of").then(|| words[words.len() - 3].parse::<u32>().ok()).flatten()
}

// ───────────── uitvoeren ─────────────

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub accepted: u32,
    pub total: Option<u32>,
    /// starting | asking | done | stalled
    pub phase: String,
}

#[derive(Debug, Clone)]
pub struct Prepared {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: Env,
    pub method: &'static str,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Problem {
    /// java_missing | java_broken | sdk_missing | cmdline_tools_missing | no_network | flutter_missing
    pub code: String,
}

/// Kiest en bereidt het commando voor. Voorkeur `sdkmanager --licenses` (één laag minder dan flutter, voorspelbare uitvoer);
/// `alternative` kiest `flutter doctor --android-licenses`, het redmiddel als sdkmanager vastloopt.
pub fn prepare(env: Env, alternative: bool) -> Result<Prepared, Vec<Problem>> {
    let mut env = env;
    let java = find_java_home(&env);
    if let Some(j) = &java {
        env.insert("JAVA_HOME".into(), j.display().to_string());
        let bin = j.join("bin").display().to_string();
        let path = env.get("PATH").cloned().unwrap_or_default();
        env.insert("PATH".into(), merge_paths(&bin, &path, if cfg!(windows) { ';' } else { ':' }));
    }
    let sdk = find_sdk_root(&env);
    if let Some(s) = &sdk {
        env.insert("ANDROID_HOME".into(), s.display().to_string());
        env.insert("ANDROID_SDK_ROOT".into(), s.display().to_string());
    }
    let p = |c: &str| Problem { code: c.into() };
    if !alternative {
        let mut problems = vec![];
        if java.is_none() {
            problems.push(p("java_missing"));
        }
        match &sdk {
            None => problems.push(p("sdk_missing")),
            Some(s) => match find_sdkmanager(s) {
                Some(m) if problems.is_empty() => {
                    return Ok(Prepared { program: m, args: vec![format!("--sdk_root={}", s.display()), "--licenses".into()], env, method: "sdkmanager" });
                }
                Some(_) => {}
                None => problems.push(p("cmdline_tools_missing")),
            },
        }
        return Err(problems);
    }
    let sep = if cfg!(windows) { ';' } else { ':' };
    let flutter = which::which_in("flutter", env.get("PATH").map(|s| s.as_str()), std::env::current_dir().unwrap_or_default()).ok();
    match flutter {
        Some(f) => Ok(Prepared { program: f, args: vec!["doctor".into(), "--android-licenses".into()], env: { let mut e = env; let _ = sep; e.entry("FLUTTER_SUPPRESS_ANALYTICS".into()).or_insert("true".into()); e }, method: "flutter" }),
        None => Err(vec![p("flutter_missing")]),
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RunResult {
    pub ok: bool,
    pub accepted: u32,
    pub total: Option<u32>,
    pub already: bool,
    pub stalled: bool,
    pub cancelled: bool,
    pub exit: Option<i32>,
    pub method: String,
    /// Ruwe uitvoer, alleen voor "Technische details".
    pub log: String,
}

pub struct RunOpts {
    /// Zonder enige uitvoer zo lang → "vastgelopen" (diagnose).
    pub stall_after: Duration,
    /// Vangnet: zo lang geen nieuwe uitvoer terwijl het proces nog draait → een batch `y`-regels schrijven en stdin sluiten.
    pub fallback_after: Duration,
    pub hard_timeout: Duration,
}

impl Default for RunOpts {
    fn default() -> Self {
        RunOpts { stall_after: Duration::from_secs(20), fallback_after: Duration::from_secs(8), hard_timeout: Duration::from_secs(600) }
    }
}

/// Voert het voorbereide commando uit. De toestemming is vóór deze aanroep gecontroleerd (zie `commands::android_licenses_accept`).
pub fn run(prep: &Prepared, opts: &RunOpts, cancel: &AtomicBool, emit: &dyn Fn(Progress)) -> RunResult {
    let mut cmd = Command::new(&prep.program);
    cmd.args(&prep.args).env_clear().envs(&prep.env).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let fail = |log: String| RunResult { ok: false, accepted: 0, total: None, already: false, stalled: false, cancelled: false, exit: None, method: prep.method.into(), log };
    // "Text file busy" (ETXTBSY) kan even optreden als het bestand net is weggeschreven; kort opnieuw proberen.
    let mut spawned = cmd.spawn();
    for _ in 0..20 {
        match &spawned {
            Err(e) if e.raw_os_error() == Some(26) => {
                std::thread::sleep(Duration::from_millis(50));
                spawned = cmd.spawn();
            }
            _ => break,
        }
    }
    let mut child = match spawned {
        Ok(c) => c,
        Err(e) => return fail(format!("could not start {}: {e}", prep.program.display())),
    };
    let mut stdin = child.stdin.take();
    let (tx, rx) = mpsc::channel::<String>();
    for mut stream in [child.stdout.take().map(|s| Box::new(s) as Box<dyn Read + Send>), child.stderr.take().map(|s| Box::new(s) as Box<dyn Read + Send>)].into_iter().flatten() {
        let tx = tx.clone();
        std::thread::spawn(move || {
            let mut buf = [0u8; 2048];
            while let Ok(n) = stream.read(&mut buf) {
                if n == 0 || tx.send(String::from_utf8_lossy(&buf[..n]).into_owned()).is_err() {
                    break;
                }
            }
        });
    }
    drop(tx);
    let mut tracker = Tracker::default();
    let started = Instant::now();
    let mut last_output = Instant::now();
    let mut stalled = false;
    let mut fell_back = false;
    let mut cancelled = false;
    let status = |t: &Tracker, phase: &str| Progress { accepted: t.accepted, total: t.total, phase: phase.into() };
    emit(status(&tracker, "starting"));
    let exit = loop {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(chunk) => {
                last_output = Instant::now();
                stalled = false;
                for step in tracker.feed(&chunk) {
                    match step {
                        Step::Answer => {
                            if let Some(w) = stdin.as_mut() {
                                let _ = w.write_all(b"y\n").and_then(|_| w.flush());
                            }
                        }
                        Step::Progress => emit(status(&tracker, if tracker.done { "done" } else { "asking" })),
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) | Err(mpsc::RecvTimeoutError::Disconnected) => {}
        }
        if let Ok(Some(st)) = child.try_wait() {
            // laatste uitvoer nog leegmaken
            while let Ok(chunk) = rx.recv_timeout(Duration::from_millis(150)) {
                tracker.feed(&chunk);
            }
            break st.code();
        }
        if cancel.load(Ordering::SeqCst) {
            cancelled = true;
            let _ = child.kill();
            break child.wait().ok().and_then(|s| s.code());
        }
        if started.elapsed() > opts.hard_timeout {
            let _ = child.kill();
            break child.wait().ok().and_then(|s| s.code());
        }
        let quiet = last_output.elapsed();
        // vangnet: de vraag is niet herkend → een batch antwoorden vooraf, daarna stdin dicht (het proces leest ze één voor één)
        if !fell_back && quiet > opts.fallback_after {
            fell_back = true;
            if let Some(mut w) = stdin.take() {
                let _ = w.write_all("y\n".repeat(60).as_bytes());
                let _ = w.flush();
            }
        }
        if quiet > opts.stall_after && !stalled {
            stalled = true;
            emit(status(&tracker, "stalled"));
        }
        if stalled && quiet > opts.stall_after * 3 {
            let _ = child.kill();
            break child.wait().ok().and_then(|s| s.code());
        }
    };
    let ok = !cancelled && exit == Some(0) && (tracker.done || tracker.accepted > 0 || tracker.already);
    RunResult {
        ok,
        accepted: tracker.accepted,
        total: tracker.total,
        already: tracker.already,
        stalled,
        cancelled,
        exit,
        method: prep.method.into(),
        log: tracker.raw().chars().rev().take(6000).collect::<Vec<_>>().into_iter().rev().collect(),
    }
}

/// Waarom loopt het vast? Controleert Java, sdkmanager en netwerk (zonder iets te wijzigen).
pub fn diagnose(env: &Env, network: bool) -> Vec<Problem> {
    let mut v = vec![];
    let p = |c: &str| Problem { code: c.into() };
    match find_java_home(env) {
        None => v.push(p("java_missing")),
        Some(j) => {
            let ok = Command::new(j.join("bin").join(exe("java"))).arg("-version").stdin(Stdio::null()).output().map(|o| o.status.success()).unwrap_or(false);
            if !ok {
                v.push(p("java_broken"));
            }
        }
    }
    match find_sdk_root(env) {
        None => v.push(p("sdk_missing")),
        Some(s) if find_sdkmanager(&s).is_none() => v.push(p("cmdline_tools_missing")),
        _ => {}
    }
    if network && !reachable("dl.google.com:443") {
        v.push(p("no_network"));
    }
    v
}

fn reachable(addr: &str) -> bool {
    use std::net::ToSocketAddrs;
    addr.to_socket_addrs().ok().and_then(|mut a| a.next()).is_some_and(|a| std::net::TcpStream::connect_timeout(&a, Duration::from_secs(4)).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    const REAL_FIRST: &str = "7 of 7 SDK package licenses not accepted.\n100% Computing updates...\nReview licenses that have not been accepted (y/N)? ";
    const LICENSE_PROMPT: &str = "\nLicense Android-SDK-License:\n---------------------------------------\nTerms and Conditions\n...\n---------------------------------------\n\nAccept? (y/N): ";

    #[test]
    fn prompts_are_recognised_in_the_variants_the_tools_print() {
        for p in ["Accept? (y/N): ", "Review licenses that have not been accepted (y/N)? ", "Continue [y/N]", "Proceed (Y/n)?", "Accept? (y/N)"] {
            assert!(ends_with_prompt(p), "{p}");
        }
        assert!(!ends_with_prompt("All SDK package licenses accepted.\n") && !ends_with_prompt("Computing updates..."));
    }

    #[test]
    fn the_tracker_counts_licenses_and_answers_each_question_once() {
        let mut t = Tracker::default();
        let s = t.feed(REAL_FIRST);
        assert_eq!(t.total, Some(7));
        assert!(s.contains(&Step::Answer));
        assert_eq!(t.accepted, 0, "the review question is not a license");
        for i in 1..=7 {
            let s = t.feed(LICENSE_PROMPT);
            assert_eq!(s.iter().filter(|x| **x == Step::Answer).count(), 1);
            assert_eq!(t.accepted, i);
            // dezelfde tekst nog eens (er komt niets nieuws) vraagt niet opnieuw om een antwoord
            assert!(!t.feed("").contains(&Step::Answer));
        }
        t.feed("\nAll SDK package licenses accepted.\n");
        assert!(t.done && !t.already);
    }

    #[test]
    fn already_accepted_needs_no_answers() {
        let mut t = Tracker::default();
        t.feed("All SDK package licenses accepted.\n");
        assert!(t.done && t.already && t.accepted == 0);
    }

    #[test]
    fn totals_are_parsed_from_both_phrasings() {
        assert_eq!(parse_total("7 of 7 SDK package licenses not accepted."), Some(7));
        assert_eq!(parse_total("3 of 6 SDK package licenses not accepted.\n"), Some(3));
        assert_eq!(parse_total("nothing"), None);
    }

    #[test]
    fn registry_values_and_variables_are_read_and_expanded() {
        let out = "\nHKEY_CURRENT_USER\\Environment\n    Path    REG_EXPAND_SZ    %USERPROFILE%\\bin;C:\\Tools\n    JAVA_HOME    REG_SZ    C:\\Java\\jdk-17\n";
        assert_eq!(parse_reg_value(out, "path").as_deref(), Some("%USERPROFILE%\\bin;C:\\Tools"));
        assert_eq!(parse_reg_value(out, "JAVA_HOME").as_deref(), Some("C:\\Java\\jdk-17"));
        assert_eq!(parse_reg_value(out, "NOPE"), None);
        let lookup = |n: &str| (n.eq_ignore_ascii_case("userprofile")).then(|| "C:\\Users\\me".to_string());
        assert_eq!(expand_vars("%USERPROFILE%\\bin;%UNKNOWN%\\x;100%", &lookup), "C:\\Users\\me\\bin;%UNKNOWN%\\x;100%");
    }

    #[test]
    fn a_path_change_after_an_install_is_visible_and_registry_values_win() {
        let process: Env = [("PATH", "C:\\Old"), ("JAVA_HOME", "C:\\OldJava"), ("USERPROFILE", "C:\\Users\\me")].iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
        let machine: Env = [("Path", "C:\\Windows;C:\\Git\\cmd"), ("JAVA_HOME", "C:\\NewJava")].iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
        let user: Env = [("Path", "%USERPROFILE%\\flutter\\bin"), ("ANDROID_HOME", "%USERPROFILE%\\sdk")].iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
        let e = combine_env(&process, &machine, &user);
        assert_eq!(e["JAVA_HOME"], "C:\\NewJava");
        assert_eq!(e["ANDROID_HOME"], "C:\\Users\\me\\sdk");
        let path = &e["PATH"];
        assert!(path.contains("C:\\Git\\cmd") && path.contains("C:\\Users\\me\\flutter\\bin") && path.contains("C:\\Old"), "{path}");
        assert_eq!(merge_paths("a;b", "B;c", ';'), "a;b;c");
    }

    fn fixture(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("studio-lic-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn java_sdk_and_sdkmanager_are_found_on_known_locations_without_path() {
        let d = fixture("find");
        let jbr = d.join("Android/Android Studio/jbr/bin");
        std::fs::create_dir_all(&jbr).unwrap();
        std::fs::write(jbr.join(exe("java")), "").unwrap();
        let sdk = d.join("local/Android/Sdk/cmdline-tools/latest/bin");
        std::fs::create_dir_all(&sdk).unwrap();
        let name = if cfg!(windows) { "sdkmanager.bat" } else { "sdkmanager" };
        std::fs::write(sdk.join(name), "").unwrap();
        let env: Env = [("PROGRAMFILES", d.display().to_string()), ("LOCALAPPDATA", d.join("local").display().to_string()), ("PATH", String::new())].into_iter().map(|(a, b)| (a.to_string(), b)).collect();
        assert_eq!(find_java_home(&env), Some(d.join("Android/Android Studio/jbr")));
        let s = find_sdk_root(&env).unwrap();
        assert_eq!(find_sdkmanager(&s), Some(sdk.join(name)));
        // JAVA_HOME wint, maar alleen als hij echt een java bevat
        let mut env2 = env.clone();
        env2.insert("JAVA_HOME".into(), d.join("nope").display().to_string());
        assert_eq!(find_java_home(&env2), Some(d.join("Android/Android Studio/jbr")));
    }

    #[test]
    fn missing_pieces_are_named() {
        let env: Env = [("PATH", String::new()), ("HOME", "/nonexistent-studio".to_string())].into_iter().map(|(a, b)| (a.to_string(), b)).collect();
        let Err(p) = prepare(env, false) else { panic!("should not be ready") };
        let codes: Vec<&str> = p.iter().map(|x| x.code.as_str()).collect();
        assert!(codes.contains(&"sdk_missing"), "{codes:?}");
    }

    #[cfg(unix)]
    fn fake_sdkmanager(dir: &Path, script: &str) -> Prepared {
        use std::os::unix::fs::PermissionsExt;
        let p = dir.join("sdkmanager");
        std::fs::write(&p, script).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        let env: Env = [("PATH".to_string(), std::env::var("PATH").unwrap_or_default())].into_iter().collect();
        Prepared { program: p, args: vec!["--licenses".into()], env, method: "sdkmanager" }
    }

    #[cfg(unix)]
    #[test]
    fn a_full_run_over_pipes_answers_every_question_and_ends_successfully() {
        let d = fixture("run");
        // Gedraagt zich als sdkmanager: eerst de samenvatting en de review-vraag, dan per licentie een vraag zonder newline.
        let prep = fake_sdkmanager(
            &d,
            "#!/bin/sh\nprintf '3 of 3 SDK package licenses not accepted.\\nReview licenses that have not been accepted (y/N)? '\nread a || exit 3\n[ \"$a\" = y ] || exit 4\nfor i in 1 2 3; do printf '\\nLicense L$i:\\n----\\nterms\\n----\\n\\nAccept? (y/N): '; read a || exit 5; [ \"$a\" = y ] || exit 6; done\nprintf '\\nAll SDK package licenses accepted.\\n'\n",
        );
        let seen = std::sync::Mutex::new(vec![]);
        let r = run(&prep, &RunOpts { stall_after: Duration::from_secs(10), fallback_after: Duration::from_secs(5), hard_timeout: Duration::from_secs(30) }, &AtomicBool::new(false), &|p| seen.lock().unwrap().push(p));
        assert!(r.ok, "{r:?}");
        assert_eq!((r.accepted, r.total, r.exit), (3, Some(3), Some(0)));
        let s = seen.lock().unwrap();
        assert!(s.iter().any(|p| p.accepted == 2 && p.total == Some(3)) && s.last().unwrap().phase == "done");
    }

    #[cfg(unix)]
    #[test]
    fn already_accepted_finishes_at_once_without_questions() {
        let d = fixture("already");
        let prep = fake_sdkmanager(&d, "#!/bin/sh\nprintf 'All SDK package licenses accepted.\\n'\n");
        let started = Instant::now();
        let r = run(&prep, &RunOpts::default(), &AtomicBool::new(false), &|_| {});
        assert!(r.ok && r.already && r.accepted == 0, "{r:?}");
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[cfg(unix)]
    #[test]
    fn an_unrecognised_question_is_still_answered_by_the_fallback_batch() {
        let d = fixture("fallback");
        // Een vraag in een vorm die niet herkend wordt ("Do you accept") en geen newline.
        let prep = fake_sdkmanager(&d, "#!/bin/sh\nprintf 'Do you accept the terms'\nread a || exit 3\n[ \"$a\" = y ] || exit 4\nprintf '\\nlicenses accepted\\n'\n");
        let r = run(&prep, &RunOpts { stall_after: Duration::from_secs(10), fallback_after: Duration::from_millis(600), hard_timeout: Duration::from_secs(20) }, &AtomicBool::new(false), &|_| {});
        assert!(r.ok && r.exit == Some(0), "{r:?}");
    }

    #[cfg(unix)]
    #[test]
    fn silence_is_reported_as_stalled_and_can_be_cancelled() {
        let d = fixture("stall");
        let prep = fake_sdkmanager(&d, "#!/bin/sh\nsleep 30\n");
        let phases = std::sync::Mutex::new(vec![]);
        let cancel = AtomicBool::new(false);
        let r = std::thread::scope(|s| {
            let h = s.spawn(|| run(&prep, &RunOpts { stall_after: Duration::from_millis(500), fallback_after: Duration::from_secs(60), hard_timeout: Duration::from_secs(60) }, &cancel, &|p| phases.lock().unwrap().push(p.phase)));
            std::thread::sleep(Duration::from_millis(1500));
            cancel.store(true, Ordering::SeqCst);
            h.join().unwrap()
        });
        assert!(r.cancelled && !r.ok && r.stalled);
        assert!(phases.lock().unwrap().iter().any(|p| p == "stalled"));
    }

    #[test]
    fn a_missing_program_is_a_plain_failure_not_a_panic() {
        let prep = Prepared { program: PathBuf::from("/nonexistent/sdkmanager"), args: vec![], env: Env::new(), method: "sdkmanager" };
        let r = run(&prep, &RunOpts::default(), &AtomicBool::new(false), &|_| {});
        assert!(!r.ok && r.log.contains("could not start"));
    }
}

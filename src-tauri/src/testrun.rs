//! AI-testrunner: een door Studio gegenereerde crawler (`flutter test`, zonder emulator) loopt alle
//! schermen af, tikt alles aan en vult formulieren met geldige, lege en foute waarden. Het rapport
//! is gekoppeld aan de code-vingerafdruk waarop de run draaide en vervalt dus bij elke wijziging.
//! Studio laat eerlijk zien wat wel en niet getest is.

use crate::error::{AppError, AppResult};
use crate::proc;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const CRAWLER_REL: &str = ".studio/studio_crawl_test.dart";
const REPORT_REL: &str = "build/studio_test/report.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Issue {
    pub kind: String,
    /// error | warning
    pub severity: String,
    pub message: String,
    #[serde(default)]
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ScreenReport {
    pub name: String,
    #[serde(default)]
    pub depth: u32,
    #[serde(default)]
    pub taps: u32,
    #[serde(default)]
    pub issues: Vec<Issue>,
    #[serde(default)]
    pub screenshot: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct TestReport {
    #[serde(default)]
    pub started_ok: bool,
    #[serde(default)]
    pub tapped: u32,
    #[serde(default)]
    pub duration_ms: u64,
    #[serde(default)]
    pub screens: Vec<ScreenReport>,
    #[serde(default)]
    pub errors: Vec<Issue>,
}

impl TestReport {
    pub fn error_count(&self) -> usize {
        self.errors.iter().chain(self.screens.iter().flat_map(|s| s.issues.iter())).filter(|i| i.severity == "error").count()
    }
    pub fn warning_count(&self) -> usize {
        self.errors.iter().chain(self.screens.iter().flat_map(|s| s.issues.iter())).filter(|i| i.severity == "warning").count()
    }
    /// Geslaagd = de app start, er is minstens één scherm bekeken en er zijn geen fouten (waarschuwingen blokkeren niet).
    pub fn passed(&self) -> bool {
        self.started_ok && !self.screens.is_empty() && self.error_count() == 0
    }
}

pub fn parse_report(json: &str) -> AppResult<TestReport> {
    Ok(serde_json::from_str(json)?)
}

/// Wat deze run NIET kan testen, afgeleid uit de packages van de app (voor de handmatige checklist).
pub fn native_features(pubspec: &str) -> Vec<String> {
    const MAP: [(&str, &str); 14] = [
        ("camera", "camera"),
        ("image_picker", "photos"),
        ("geolocator", "location"),
        ("location", "location"),
        ("firebase_messaging", "push"),
        ("flutter_local_notifications", "push"),
        ("in_app_purchase", "payments"),
        ("purchases_flutter", "payments"),
        ("local_auth", "biometrics"),
        ("flutter_blue_plus", "bluetooth"),
        ("google_mobile_ads", "ads"),
        ("url_launcher", "links"),
        ("share_plus", "sharing"),
        ("record", "microphone"),
    ];
    let deps: Vec<&str> = pubspec.lines().filter_map(|l| l.strip_prefix("  ").and_then(|r| r.split(':').next())).collect();
    let mut out: Vec<String> = vec![];
    for (pkg, feature) in MAP {
        if deps.contains(&pkg) && !out.iter().any(|f| f == feature) {
            out.push(feature.into());
        }
    }
    out
}

pub fn ensure_crawler(dir: &Path, package: &str) -> AppResult<PathBuf> {
    let p = dir.join(CRAWLER_REL);
    std::fs::create_dir_all(dir.join(".studio"))?;
    let src = include_str!("../templates/studio_crawl_test.dart").replace("__PACKAGE__", package);
    std::fs::write(&p, crate::templates::adapt_material(dir, &src))?;
    // Het gegenereerde testbestand hoort niet in git (en mag de code-vingerafdruk niet beïnvloeden).
    let gi = dir.join(".gitignore");
    let text = std::fs::read_to_string(&gi).unwrap_or_default();
    if !text.lines().any(|l| l.trim() == CRAWLER_REL) {
        std::fs::write(&gi, format!("{}{}\n# Studio test crawler (generated)\n{CRAWLER_REL}\n", text, if text.ends_with('\n') || text.is_empty() { "" } else { "\n" }))?;
    }
    Ok(p)
}

pub fn package_name(dir: &Path) -> Option<String> {
    std::fs::read_to_string(dir.join("pubspec.yaml")).ok()?.lines().find_map(|l| l.strip_prefix("name:")).map(|n| n.trim().to_string())
}

pub struct RunOutcome {
    pub report: Option<TestReport>,
    pub log: String,
}

/// Draait de crawler. `on_line` krijgt de uitvoer van `flutter test` regel voor regel.
pub fn run(flutter: &Path, dir: &Path, on_line: &mut dyn FnMut(&str)) -> AppResult<RunOutcome> {
    use std::io::{BufRead, BufReader};
    use std::process::Stdio;
    let package = package_name(dir).ok_or_else(|| AppError::Invalid("pubspec.yaml has no name".into()))?;
    ensure_crawler(dir, &package)?;
    let _ = std::fs::remove_file(dir.join(REPORT_REL));

    let mut cmd = proc::command(flutter);
    // De testrun doet nooit echte aankopen: een project met aankopen gebruikt dan de FakePurchaseProvider.
    cmd.args(["test", "--dart-define=STUDIO_FAKE_PURCHASES=true", CRAWLER_REL]).current_dir(dir).stderr(Stdio::piped()).stdout(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| AppError::CommandFailed(format!("flutter test: {e}")))?;
    let stderr = child.stderr.take();
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let readers: Vec<_> = [child.stdout.take().map(|s| Box::new(s) as Box<dyn std::io::Read + Send>), stderr.map(|s| Box::new(s) as Box<dyn std::io::Read + Send>)]
        .into_iter()
        .flatten()
        .map(|s| {
            let tx = tx.clone();
            std::thread::spawn(move || {
                for l in BufReader::new(s).lines().map_while(Result::ok) {
                    let _ = tx.send(crate::logging::redact(&l));
                }
            })
        })
        .collect();
    drop(tx);
    let deadline = std::time::Instant::now() + Duration::from_secs(600);
    let mut log = String::new();
    loop {
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(l) => {
                on_line(&l);
                log.push_str(&l);
                log.push('\n');
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            Err(_) => {}
        }
        if std::time::Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(AppError::Timeout("flutter test".into()));
        }
    }
    for r in readers {
        let _ = r.join();
    }
    let _ = child.wait();
    let report = std::fs::read_to_string(dir.join(REPORT_REL)).ok().and_then(|t| parse_report(&t).ok());
    Ok(RunOutcome { report, log })
}

pub fn read_report(dir: &Path) -> Option<TestReport> {
    std::fs::read_to_string(dir.join(REPORT_REL)).ok().and_then(|t| parse_report(&t).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{"crawler":1,"startedOk":true,"tapped":7,"durationMs":4200,
      "screens":[{"name":"Friday Scores","depth":0,"taps":5,"issues":[{"kind":"dead_button","severity":"warning","message":"A TextButton seems to do nothing"}],"screenshot":"screen_0.png"},
                 {"name":"Settings","depth":1,"taps":2,"issues":[],"screenshot":null}],
      "errors":[]}"#;

    #[test]
    fn parses_reports_and_counts() {
        let r = parse_report(SAMPLE).unwrap();
        assert_eq!((r.screens.len(), r.tapped, r.error_count(), r.warning_count()), (2, 7, 0, 1));
        assert!(r.passed(), "warnings alone must not fail a run");
    }

    #[test]
    fn crashes_dead_screens_and_start_failures_fail_the_run() {
        let mut r = parse_report(SAMPLE).unwrap();
        r.screens[1].issues.push(Issue { kind: "crash".into(), severity: "error".into(), message: "Tapping a TextButton caused an error".into(), detail: String::new() });
        assert!(!r.passed());
        let r = parse_report(r#"{"startedOk":false,"screens":[],"errors":[{"kind":"crash","severity":"error","message":"The app could not start"}]}"#).unwrap();
        assert!(!r.passed() && r.error_count() == 1);
        // een run zonder één bekeken scherm telt niet als geslaagd
        assert!(!parse_report(r#"{"startedOk":true,"screens":[]}"#).unwrap().passed());
        assert!(parse_report("not json").is_err());
    }

    #[test]
    fn detects_native_features_to_test_by_hand() {
        let pubspec = "dependencies:\n  flutter:\n    sdk: flutter\n  camera: ^0.10.0\n  image_picker: ^1.0.0\n  firebase_messaging: ^15.0.0\n  provider: ^6.0.0\n";
        assert_eq!(native_features(pubspec), ["camera", "photos", "push"]);
        assert!(native_features("dependencies:\n  flutter:\n    sdk: flutter\n").is_empty());
    }

    #[test]
    fn crawler_is_written_with_the_package_name_and_ignored_by_git() {
        let d = std::env::temp_dir().join(format!("studio-crawl-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("pubspec.yaml"), "name: friday_scores\nversion: 1.0.0+1\n").unwrap();
        std::fs::write(d.join(".gitignore"), "build/").unwrap();
        assert_eq!(package_name(&d).as_deref(), Some("friday_scores"));
        let p = ensure_crawler(&d, "friday_scores").unwrap();
        let src = std::fs::read_to_string(p).unwrap();
        assert!(src.contains("package:friday_scores/main.dart") && !src.contains("__PACKAGE__"));
        ensure_crawler(&d, "friday_scores").unwrap();
        let gi = std::fs::read_to_string(d.join(".gitignore")).unwrap();
        assert_eq!(gi.matches(CRAWLER_REL).count(), 1, "idempotent");
        assert!(gi.starts_with("build/\n"));
    }

    #[cfg(unix)]
    #[test]
    fn runs_flutter_test_and_reads_the_report() {
        use std::os::unix::fs::PermissionsExt;
        let d = std::env::temp_dir().join(format!("studio-run-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("pubspec.yaml"), "name: demo\n").unwrap();
        let fake = d.join("flutter");
        std::fs::write(&fake, format!("#!/bin/sh\necho \"args: $@\"\necho 'token=supersecret123'\nmkdir -p build/studio_test\ncat > build/studio_test/report.json <<'EOF'\n{SAMPLE}\nEOF\nexit 1\n")).unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut lines = vec![];
        let out = run(&fake, &d, &mut |l| lines.push(l.to_string())).unwrap();
        // exitcode 1 (een mislukte test) is geen reden om het rapport te negeren
        assert!(out.report.unwrap().passed());
        assert!(lines[0].contains(".studio/studio_crawl_test.dart"));
        assert!(out.log.contains("token=[redacted]") && !out.log.contains("supersecret"));
        // zonder rapport: geen rapport (de UI toont de log)
        std::fs::write(&fake, "#!/bin/sh\necho boom\nexit 1\n").unwrap();
        assert!(run(&fake, &d, &mut |_| {}).unwrap().report.is_none());
    }

    /// Echte Flutter: `cargo test real_crawler -- --ignored --nocapture`. Draait de crawler tegen elk sjabloon.
    #[test]
    #[ignore]
    fn real_crawler() {
        use crate::db::Db;
        use crate::projects::{create_project, CreateRequest, Tools};
        let (Some(flutter), Some(git)) = (crate::proc::find("flutter"), crate::proc::find("git")) else { panic!("flutter and git on PATH") };
        let root = std::env::temp_dir().join(format!("studio-crawl-real-{}", uuid::Uuid::new_v4()));
        let db = Db::open_in_memory().unwrap();
        let tools = Tools { flutter: flutter.clone(), git };
        for template in ["scoreboard", "checklist", "tracker", "blank"] {
            let req = CreateRequest { name: format!("Crawl {template}"), template: template.into(), description: String::new(), org: "com.example".into() };
            let p = create_project(&db, &tools, &root, &req, &|_| {}).unwrap();
            let dir = std::path::Path::new(&p.path);
            let out = run(&flutter, dir, &mut |_| {}).unwrap();
            match &out.report {
                Some(r) => {
                    println!("[{template}] passed={} screens={} taps={} errors={} warnings={}", r.passed(), r.screens.len(), r.tapped, r.error_count(), r.warning_count());
                    for s in &r.screens {
                        for i in &s.issues {
                            println!("   [{}] {} {}: {}", s.name, i.severity, i.kind, i.message);
                        }
                    }
                    for i in &r.errors {
                        println!("   [global] {} {}: {} {}", i.severity, i.kind, i.message, i.detail);
                    }
                }
                None => println!("[{template}] NO REPORT. Log tail:\n{}", out.log.lines().rev().take(25).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n")),
            }
            assert!(out.report.is_some(), "{template}: the crawler must produce a report");
        }
    }

    /// Echte Flutter: `STUDIO_TEST_PROJECT=<map> cargo test real_crawler_project -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn real_crawler_project() {
        let dir = std::path::PathBuf::from(std::env::var("STUDIO_TEST_PROJECT").expect("STUDIO_TEST_PROJECT"));
        let flutter = crate::proc::find("flutter").expect("flutter on PATH");
        let out = run(&flutter, &dir, &mut |_| {}).unwrap();
        let r = out.report.expect("report");
        println!("passed={} screens={:?} taps={} errors={} warnings={}", r.passed(), r.screens.iter().map(|s| format!("{} (depth {}, {} taps)", s.name, s.depth, s.taps)).collect::<Vec<_>>(), r.tapped, r.error_count(), r.warning_count());
        for s in &r.screens {
            for i in &s.issues {
                println!("   [{}] {} {}: {}", s.name, i.severity, i.kind, i.message);
            }
        }
        println!("screenshots: {:?}", r.screens.iter().filter_map(|s| s.screenshot.clone()).collect::<Vec<_>>());
    }
}

//! Controle na een run (fase 7 en 8): `flutter analyze` (alleen op wat geraakt kan zijn), relevante tests, gerichte
//! reparatie (maximaal twee pogingen) en daarna terugdraaien naar de momentopname van vóór de run.

use crate::index::CodeIndex;
use crate::scope::Level;
use std::path::Path;

pub const MAX_REPAIRS: u32 = 2;

#[derive(Debug, Clone, PartialEq)]
pub struct Issue {
    pub file: String,
    pub line: u32,
    pub message: String,
}

/// Regels van `flutter analyze`: `  error • Message • lib/a.dart:3:5 • code` (oudere versies gebruiken ` - `).
/// Alleen fouten en waarschuwingen tellen; info-meldingen niet.
pub fn parse_analyze(output: &str) -> Vec<Issue> {
    let mut out = vec![];
    for raw in output.lines() {
        let line = raw.trim();
        let sev = ["error", "warning"].iter().find(|s| line.starts_with(&format!("{s} ")));
        let Some(sev) = sev else { continue };
        let rest = line[sev.len()..].trim_start_matches([' ', '•', '-']);
        let parts: Vec<&str> = rest.split(" • ").collect();
        let parts: Vec<&str> = if parts.len() >= 3 { parts } else { rest.split(" - ").collect() };
        if parts.len() < 3 {
            continue;
        }
        let loc = parts[parts.len() - 2];
        let mut it = loc.rsplitn(3, ':');
        let _col = it.next();
        let ln = it.next().and_then(|n| n.parse::<u32>().ok());
        let file = it.next();
        if let (Some(file), Some(ln)) = (file, ln) {
            let message = parts[..parts.len() - 2].join(" - ");
            out.push(Issue { file: file.replace('\\', "/"), line: ln, message: format!("{sev}: {message}") });
        }
    }
    out
}

/// Wat geanalyseerd moet worden. Klein/Scherm: de gewijzigde bestanden en hun directe gebruikers (sneller).
/// Gedeeld/Project of onbekend: leeg = het hele project.
pub fn analyze_targets(level: Level, changed: &[String], idx: &CodeIndex) -> Vec<String> {
    if level >= Level::Shared {
        return vec![];
    }
    let mut t: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for c in changed.iter().filter(|c| c.ends_with(".dart")) {
        t.insert(c.clone());
        // Een gewijzigd bestand kan alleen zijn directe gebruikers breken.
        t.extend(idx.files.values().filter(|n| n.imports.contains(c)).map(|n| n.path.clone()));
    }
    t.into_iter().collect()
}

/// Testbestanden die bij de wijziging horen: de rooktest altijd, de rest als ze een gewijzigd bestand of een
/// gewijzigde featuremap noemen. Bij Gedeeld/Project alle tests.
pub fn relevant_tests(dir: &Path, level: Level, changed: &[String]) -> Vec<String> {
    let mut all = vec![];
    fn walk(d: &Path, root: &Path, out: &mut Vec<String>) {
        let Ok(rd) = std::fs::read_dir(d) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, root, out);
            } else if p.to_string_lossy().ends_with("_test.dart") {
                if let Ok(r) = p.strip_prefix(root) {
                    out.push(r.to_string_lossy().replace('\\', "/"));
                }
            }
        }
    }
    walk(&dir.join("test"), dir, &mut all);
    all.sort();
    if level >= Level::Shared {
        return all;
    }
    let keys: Vec<String> = changed
        .iter()
        .filter(|c| c.starts_with("lib/"))
        .flat_map(|c| {
            let stem = c.rsplit('/').next().unwrap_or(c).trim_end_matches(".dart").to_string();
            let feature = c.strip_prefix("lib/features/").and_then(|r| r.split('/').next()).map(|f| format!("features/{f}/"));
            [Some(stem), feature].into_iter().flatten()
        })
        .collect();
    all.into_iter()
        .filter(|t| {
            t.ends_with("widget_test.dart")
                || t.contains("smoke")
                || std::fs::read_to_string(dir.join(t)).map(|s| keys.iter().any(|k| s.contains(k.as_str()))).unwrap_or(false)
        })
        .collect()
}

/// Draait de controles echt: `flutter analyze` (op de relevante bestanden) en daarna de bijbehorende tests.
/// Lukt het starten niet (geen internet voor `pub get`, geen Flutter), dan blokkeert dat de gebruiker nooit.
pub fn run_checks(dir: &Path, flutter: &Path, level: Level, changed: &[String]) -> Vec<Issue> {
    use std::time::Duration;
    let idx = CodeIndex::build(dir).unwrap_or_default();
    let mut targets = analyze_targets(level, changed, &idx);
    targets.retain(|t| dir.join(t).exists());
    let only_dart = changed.iter().any(|c| c.ends_with(".dart")) && !changed.iter().any(|c| c == "pubspec.yaml");
    let mut args: Vec<&str> = vec!["analyze"];
    if only_dart && !targets.is_empty() {
        args.extend(targets.iter().map(String::as_str));
    }
    if let Ok(out) = crate::proc::run(flutter, &args, Some(dir), Duration::from_secs(600)) {
        let issues = parse_analyze(&out.combined());
        if !issues.is_empty() {
            return issues;
        }
    }
    let tests = relevant_tests(dir, level, changed);
    if tests.is_empty() {
        return vec![];
    }
    let mut targs: Vec<&str> = vec!["test"];
    targs.extend(tests.iter().map(String::as_str));
    match crate::proc::run(flutter, &targs, Some(dir), Duration::from_secs(900)) {
        Ok(out) if !out.ok() && out.status.is_some() => {
            let text = out.combined();
            let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
            // Begin bij de eerste echte foutmelding van de test; anders de laatste regels.
            let from = lines.iter().position(|l| l.contains("EXCEPTION CAUGHT") || l.contains("Expected:")).unwrap_or(lines.len().saturating_sub(12));
            let tail: Vec<&str> = lines[from..].iter().take(12).copied().collect();
            vec![Issue { file: tests[0].clone(), line: 0, message: format!("tests failed:\n{}", tail.join("\n")) }]
        }
        _ => vec![],
    }
}

pub fn repair_prompt(issues: &[Issue], attempt: u32) -> String {
    let list: Vec<String> = issues.iter().take(12).map(|i| if i.line > 0 { format!("- {}:{} {}", i.file, i.line, i.message) } else { format!("- {} {}", i.file, i.message) }).collect();
    format!(
        "Studio checked your last change and found problems (repair attempt {attempt} of {MAX_REPAIRS}):\n{}\n\
         Fix exactly these problems with the smallest possible edit. Do not change anything else and do not add features. \
         You may edit the files named above even if they are outside the original scope.",
        list.join("\n")
    )
}

#[derive(Debug, PartialEq)]
pub struct GateResult {
    pub initial_issues: usize,
    pub repairs: u32,
    pub rolled_back: bool,
    pub remaining: Vec<Issue>,
}

/// De herstel-lus, los van processen zodat hij te testen is: controleer, repareer (max. `MAX_REPAIRS`), draai terug.
pub fn gate(check: &mut dyn FnMut() -> Vec<Issue>, repair: &mut dyn FnMut(&[Issue], u32) -> bool, rollback: &mut dyn FnMut()) -> GateResult {
    let mut issues = check();
    let initial = issues.len();
    let mut repairs = 0;
    while !issues.is_empty() && repairs < MAX_REPAIRS {
        repairs += 1;
        if !repair(&issues, repairs) {
            break; // geen engine of geannuleerd: niet blijven proberen
        }
        issues = check();
    }
    let rolled_back = !issues.is_empty();
    if rolled_back {
        rollback();
    }
    GateResult { initial_issues: initial, repairs, rolled_back, remaining: issues }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::fixture;

    const OUT: &str = "Analyzing demo...\n\n  error • Undefined name 'foo' • lib/features/home/home_screen.dart:12:5 • undefined_identifier\n\
        warning • Unused import: 'x.dart' • lib/core/widgets/empty_state.dart:1:8 • unused_import\n   info • Use const • lib/a.dart:3:1 • prefer_const\n\
        warning - Old style message - lib\\b.dart:7:2 - unused_local_variable\n3 issues found.\n";

    #[test]
    fn parses_errors_and_warnings_but_not_infos() {
        let i = parse_analyze(OUT);
        assert_eq!(i.len(), 3);
        assert_eq!(i[0], Issue { file: "lib/features/home/home_screen.dart".into(), line: 12, message: "error: Undefined name 'foo'".into() });
        assert_eq!(i[2].file, "lib/b.dart");
        assert!(parse_analyze("No issues found! (ran in 1.2s)").is_empty());
    }

    #[test]
    fn analyzes_only_what_a_small_change_can_break() {
        let d = fixture("quality");
        let idx = CodeIndex::build(&d).unwrap();
        let t = analyze_targets(Level::Screen, &["lib/features/home/widgets/card.dart".into()], &idx);
        assert_eq!(t, vec!["lib/features/home/home_screen.dart", "lib/features/home/widgets/card.dart"]);
        let t = analyze_targets(Level::Small, &["lib/core/theme/app_theme.dart".into()], &idx);
        assert!(t.contains(&"lib/features/settings/settings_screen.dart".to_string()), "direct users are checked too");
        assert!(!t.contains(&"lib/main.dart".to_string()), "but not the whole chain");
        assert!(analyze_targets(Level::Shared, &["lib/core/theme/app_theme.dart".into()], &idx).is_empty(), "empty = whole project");
        assert!(analyze_targets(Level::Small, &["pubspec.yaml".into()], &idx).is_empty());
    }

    #[test]
    fn picks_the_tests_that_belong_to_the_change() {
        let d = fixture("tests");
        let w = |p: &str, t: &str| {
            std::fs::create_dir_all(d.join(p).parent().unwrap()).unwrap();
            std::fs::write(d.join(p), t).unwrap();
        };
        w("test/widget_test.dart", "void main(){}");
        w("test/home_test.dart", "import 'package:demo/features/home/home_screen.dart';");
        w("test/settings_test.dart", "import 'package:demo/features/settings/settings_screen.dart';");
        let home = relevant_tests(&d, Level::Screen, &["lib/features/home/widgets/card.dart".into()]);
        assert_eq!(home, vec!["test/home_test.dart", "test/widget_test.dart"]);
        assert_eq!(relevant_tests(&d, Level::Shared, &[]).len(), 3);
    }

    #[test]
    fn repair_loop_stops_after_a_fix_or_two_attempts_and_then_rolls_back() {
        let mut issue = || vec![Issue { file: "lib/a.dart".into(), line: 1, message: "error: x".into() }];
        // Eén reparatie lost het op.
        let mut n = 0;
        let mut rb = 0;
        let r = gate(&mut || { n += 1; if n >= 2 { vec![] } else { vec![Issue { file: "lib/a.dart".into(), line: 1, message: "error: x".into() }] } }, &mut |_, _| true, &mut || rb += 1);
        assert_eq!((r.initial_issues, r.repairs, r.rolled_back, rb), (1, 1, false, 0));
        // Nooit opgelost: precies twee pogingen, dan terugdraaien.
        let mut attempts = vec![];
        let mut rb = 0;
        let r = gate(&mut issue, &mut |_, a| { attempts.push(a); true }, &mut || rb += 1);
        assert_eq!((attempts, r.repairs, r.rolled_back, rb, r.remaining.len()), (vec![1, 2], 2, true, 1, 1));
        // Schoon vanaf het begin: niets te doen.
        let mut rb = 0;
        let r = gate(&mut Vec::new, &mut |_, _| panic!("no repair needed"), &mut || rb += 1);
        assert_eq!((r.repairs, r.rolled_back, rb), (0, false, 0));
        // Engine niet beschikbaar: direct terugdraaien in plaats van blijven proberen.
        let r = gate(&mut issue, &mut |_, _| false, &mut || {});
        assert_eq!((r.repairs, r.rolled_back), (1, true));
    }

    #[test]
    fn repair_prompt_names_files_and_limits_the_edit() {
        let p = repair_prompt(&[Issue { file: "lib/a.dart".into(), line: 4, message: "error: boom".into() }], 1);
        assert!(p.contains("lib/a.dart:4 error: boom") && p.contains("attempt 1 of 2") && p.contains("smallest possible edit"));
    }

    /// Met het echte Flutter: de analyze-uitvoer wordt echt herkend, alleen de geraakte bestanden worden geanalyseerd en
    /// een kapotte test wordt gevonden.
    #[test]
    #[ignore]
    fn real_checks_find_a_broken_file_and_a_broken_test() {
        use crate::projects::{create_project, CreateRequest, Tools};
        let flutter = crate::proc::find("flutter").expect("flutter on PATH");
        let git = crate::proc::find("git").unwrap();
        let root = std::env::temp_dir().join(format!("studio-quality-{}", uuid::Uuid::new_v4()));
        let db = crate::db::Db::open_in_memory().unwrap();
        let req = CreateRequest { name: "Quality Demo".into(), template: "scoreboard".into(), description: "x".into(), org: "com.example".into() };
        let p = create_project(&db, &Tools { flutter: flutter.clone(), git }, &root.join("projects"), &req, &|_| {}).unwrap();
        let dir = std::path::PathBuf::from(&p.path);

        let clean = run_checks(&dir, &flutter, Level::Screen, &["lib/features/scoreboard/scoreboard_screen.dart".into()]);
        println!("clean project: {clean:?}");
        assert!(clean.is_empty(), "{clean:?}");

        let file = dir.join("lib/features/scoreboard/widgets/player_card.dart");
        let original = std::fs::read_to_string(&file).unwrap();
        std::fs::write(&file, format!("{original}\nfinal int broken = 'not an int';\n")).unwrap();
        let t = std::time::Instant::now();
        let issues = run_checks(&dir, &flutter, Level::Screen, &["lib/features/scoreboard/widgets/player_card.dart".into()]);
        println!("broken file after {:?}: {issues:?}", t.elapsed());
        assert!(issues.iter().any(|i| i.file == "lib/features/scoreboard/widgets/player_card.dart" && i.line > 0 && i.message.starts_with("error")), "{issues:?}");

        std::fs::write(&file, original).unwrap();
        let widget_test = dir.join("test/widget_test.dart");
        let t0 = std::fs::read_to_string(&widget_test).unwrap();
        std::fs::write(&widget_test, t0.replace("expect(", "expect(false, true); expect(")).unwrap();
        let issues = run_checks(&dir, &flutter, Level::Screen, &["lib/features/scoreboard/scoreboard_screen.dart".into()]);
        println!("broken test: {:?}", issues.iter().map(|i| (&i.file, i.line, i.message.chars().take(80).collect::<String>())).collect::<Vec<_>>());
        assert!(issues.iter().any(|i| i.message.starts_with("tests failed") || i.message.starts_with("error")), "{issues:?}");
    }
}

//! Evaluatieset (fase 11d): tien vaste opdrachten (klein tot complex) uit `evals/tasks.json`, die per model door de échte
//! Studio-keten lopen (werkgebied, modelkeuze, controle, reparatie) en echte cijfers geven: slagingspercentage, tokens en
//! kosten per klasse en model. Zo stel je de mapping in `engines/*.json` af met data in plaats van met gevoel.
//!
//! Draaien (kost echte tokens): `STUDIO_EVAL_ALIASES=haiku,sonnet,opus cargo test real_eval -- --ignored --nocapture`.
//! Resultaten komen als regels in `STUDIO_EVAL_OUT` (standaard /tmp/studio-eval/results.jsonl); een gelijke (taak, model) wordt
//! overgeslagen, dus een onderbroken run kan gewoon opnieuw gestart worden.

use crate::modelsel::EngineConfig;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct TaskFile {
    pub tasks: Vec<Task>,
}

#[derive(Debug, Deserialize)]
pub struct Setup {
    pub file: String,
    pub replace: [String; 2],
}

#[derive(Debug, Deserialize)]
pub struct Hidden {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Check {
    Regex { path: String, pattern: String },
    Exists { path: String },
    /// Ergens onder `path` staat een bestand of map waarvan de naam bij het patroon past (modellen kiezen zelf namen).
    PathMatches { path: String, pattern: String },
    Absent { path: String, pattern: String },
    FlutterTest { path: String },
}

#[derive(Debug, Deserialize)]
pub struct Task {
    pub id: String,
    /// De klasse die we verwachten (om de classificatie te beoordelen).
    pub expect: String,
    pub prompt: String,
    #[serde(default)]
    pub setup: Vec<Setup>,
    #[serde(default, rename = "hiddenTests")]
    pub hidden_tests: Vec<Hidden>,
    pub checks: Vec<Check>,
}

pub fn load_tasks() -> TaskFile {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../evals/tasks.json");
    serde_json::from_str(&std::fs::read_to_string(p).expect("evals/tasks.json")).expect("valid tasks.json")
}

fn dart_files(p: &Path, out: &mut Vec<std::path::PathBuf>) {
    if p.is_file() {
        out.push(p.to_path_buf());
    } else if let Ok(rd) = std::fs::read_dir(p) {
        for e in rd.flatten() {
            dart_files(&e.path(), out);
        }
    }
}

/// De tekst van een bestand (welk type dan ook), of van alle Dart-bestanden in een map.
fn text_of(dir: &Path, rel: &str) -> String {
    let target = dir.join(rel);
    if target.is_file() {
        return std::fs::read_to_string(&target).unwrap_or_default();
    }
    let mut files = vec![];
    dart_files(&target, &mut files);
    files.sort();
    files.iter().filter(|f| f.extension().is_some_and(|e| e == "dart")).filter_map(|f| std::fs::read_to_string(f).ok()).collect::<Vec<_>>().join("\n")
}

/// Alle paden onder `rel` (relatief), voor controles op "er is ergens een map of bestand met zo'n naam".
fn paths_under(dir: &Path, rel: &str) -> Vec<String> {
    let root = dir.join(rel);
    let mut out = vec![];
    fn walk(root: &Path, d: &Path, out: &mut Vec<String>) {
        let Ok(rd) = std::fs::read_dir(d) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if let Ok(r) = p.strip_prefix(root) {
                out.push(r.to_string_lossy().replace('\\', "/"));
            }
            if p.is_dir() {
                walk(root, &p, out);
            }
        }
    }
    walk(&root, &root, &mut out);
    out
}

/// Voert één controle uit. `flutter` is alleen nodig voor `flutterTest`.
pub fn run_check(dir: &Path, flutter: Option<&Path>, c: &Check) -> bool {
    let re = |p: &str| regex_lite::Regex::new(p).expect("valid check pattern");
    match c {
        Check::Regex { path, pattern } => re(pattern).is_match(&text_of(dir, path)),
        Check::Absent { path, pattern } => !re(pattern).is_match(&text_of(dir, path)),
        Check::Exists { path } => dir.join(path).exists(),
        Check::PathMatches { path, pattern } => paths_under(dir, path).iter().any(|p| re(pattern).is_match(p)),
        Check::FlutterTest { path } => flutter
            .and_then(|f| crate::proc::run(f, &["test", path], Some(dir), std::time::Duration::from_secs(600)).ok())
            .map(|o| o.ok())
            .unwrap_or(false),
    }
}

/// Resultaat van één (taak, model)-run.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RunResult {
    pub task: String,
    pub expect: String,
    /// Klasse die Studio zelf aan de opdracht gaf.
    pub class: String,
    pub alias: String,
    pub tier_of_class: String,
    pub kept: bool,
    pub checks_ok: bool,
    pub clean: bool,
    pub repairs: i64,
    pub cost_usd: f64,
    pub tokens: i64,
    pub seconds: f64,
}

impl RunResult {
    pub fn success(&self) -> bool {
        self.kept && self.checks_ok
    }
}

#[derive(Debug, Default, PartialEq)]
pub struct Cell {
    pub runs: usize,
    pub success: usize,
    pub clean: usize,
    pub cost: f64,
    pub tokens: i64,
    pub seconds: f64,
}

impl Cell {
    fn add(&mut self, r: &RunResult) {
        self.runs += 1;
        self.success += r.success() as usize;
        self.clean += (r.success() && r.clean) as usize;
        self.cost += r.cost_usd;
        self.tokens += r.tokens;
        self.seconds += r.seconds;
    }
}

/// Per klasse en model: aantal, slagingspercentage, meteen goed, gemiddelde kosten en tokens.
pub fn by_class_and_model(results: &[RunResult]) -> BTreeMap<(String, String), Cell> {
    let mut m: BTreeMap<(String, String), Cell> = BTreeMap::new();
    for r in results {
        m.entry((r.class.clone(), r.alias.clone())).or_default().add(r);
    }
    m
}

/// Wat Studio automatisch zou doen: per taak het model van de klasse. Vergeleken met "altijd het sterkste" en "altijd het lichtste".
pub fn auto_vs_baselines(results: &[RunResult], cfg: &EngineConfig) -> BTreeMap<&'static str, Cell> {
    let alias_of = |tier: &str| cfg.tier_index(tier).and_then(|i| cfg.tiers[i].models.first().cloned()).unwrap_or_default();
    let (light, strong) = (alias_of(&cfg.tiers[0].id), alias_of(&cfg.tiers[cfg.strongest()].id));
    let mut out: BTreeMap<&'static str, Cell> = BTreeMap::new();
    let mut tasks: Vec<&str> = results.iter().map(|r| r.task.as_str()).collect();
    tasks.sort();
    tasks.dedup();
    for t in tasks {
        let runs: Vec<&RunResult> = results.iter().filter(|r| r.task == t).collect();
        let Some(any) = runs.first() else { continue };
        let auto_alias = alias_of(cfg.class_map.get(&any.class).map(String::as_str).unwrap_or(&cfg.tiers[0].id));
        for (name, alias) in [("auto", &auto_alias), ("always-strongest", &strong), ("always-lightest", &light)] {
            if let Some(r) = runs.iter().find(|r| &r.alias == alias) {
                out.entry(name).or_default().add(r);
            }
        }
    }
    out
}

pub fn report(results: &[RunResult], cfg: &EngineConfig) -> String {
    let mut s = String::from("## Per class and model\n\n| class | model | runs | success | clean first time | avg cost | avg tokens | avg time |\n|---|---|---|---|---|---|---|---|\n");
    for ((class, alias), c) in by_class_and_model(results) {
        let n = c.runs as f64;
        s.push_str(&format!(
            "| {class} | {alias} | {} | {:.0}% | {:.0}% | ${:.3} | {:.0}k | {:.0}s |\n",
            c.runs,
            100.0 * c.success as f64 / n,
            100.0 * c.clean as f64 / n,
            c.cost / n,
            c.tokens as f64 / n / 1000.0,
            c.seconds / n
        ));
    }
    s.push_str("\n## Automatic choice versus fixed models (same tasks)\n\n| strategy | tasks | success | total cost | total tokens |\n|---|---|---|---|---|\n");
    for (name, c) in auto_vs_baselines(results, cfg) {
        s.push_str(&format!("| {name} | {} | {:.0}% | ${:.2} | {:.0}k |\n", c.runs, 100.0 * c.success as f64 / c.runs.max(1) as f64, c.cost, c.tokens as f64 / 1000.0));
    }
    s.push_str("\n## Per task\n\n| task | expected | studio said | haiku/sonnet/opus ... (success, cost) |\n|---|---|---|---|\n");
    let mut tasks: Vec<&str> = results.iter().map(|r| r.task.as_str()).collect();
    tasks.sort();
    tasks.dedup();
    for t in tasks {
        let runs: Vec<&RunResult> = results.iter().filter(|r| r.task == t).collect();
        let cells: Vec<String> = runs.iter().map(|r| format!("{} {} ${:.3}", r.alias, if r.success() { "ok" } else { "FAIL" }, r.cost_usd)).collect();
        s.push_str(&format!("| {t} | {} | {} | {} |\n", runs[0].expect, runs[0].class, cells.join(" · ")));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_task_file_is_valid_and_its_setups_apply_to_the_template() {
        let f = load_tasks();
        assert_eq!(f.tasks.len(), 13);
        let mut ids: Vec<&str> = f.tasks.iter().map(|t| t.id.as_str()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 13, "unique ids");
        let classes: Vec<&str> = f.tasks.iter().map(|t| t.expect.as_str()).collect();
        for c in ["trivial", "normal", "complex"] {
            assert!(classes.contains(&c), "{c} is covered");
        }
        let tpl = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates/app/scoreboard");
        for t in &f.tasks {
            for s in &t.setup {
                let text = std::fs::read_to_string(tpl.join(&s.file)).unwrap_or_else(|_| panic!("{}: {}", t.id, s.file));
                assert!(text.contains(&s.replace[0]), "{}: setup text must exist in the template", t.id);
            }
            for c in &t.checks {
                if let Check::Regex { pattern, .. } | Check::Absent { pattern, .. } = c {
                    regex_lite::Regex::new(pattern).unwrap_or_else(|e| panic!("{}: {e}", t.id));
                }
            }
        }
    }

    #[test]
    fn checks_look_in_files_and_folders() {
        let d = std::env::temp_dir().join(format!("studio-evalcheck-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(d.join("lib/features/x")).unwrap();
        std::fs::write(d.join("lib/features/x/a.dart"), "final total = 1; // Game Night\n").unwrap();
        std::fs::write(d.join("lib/b.dart"), "var s = p.score;\n").unwrap();
        let rx = |path: &str, pattern: &str| Check::Regex { path: path.into(), pattern: pattern.into() };
        assert!(run_check(&d, None, &rx("lib/features", "(?i)total")));
        assert!(run_check(&d, None, &rx("lib/features/x/a.dart", "Game Night")));
        assert!(!run_check(&d, None, &rx("lib/features", "settings")));
        assert!(run_check(&d, None, &Check::Exists { path: "lib/features/x".into() }));
        assert!(!run_check(&d, None, &Check::Exists { path: "lib/features/y".into() }));
        assert!(run_check(&d, None, &Check::PathMatches { path: "lib/features".into(), pattern: "(?i)^x(/|$)".into() }));
        assert!(!run_check(&d, None, &Check::PathMatches { path: "lib/features".into(), pattern: "(?i)tourn".into() }));
        std::fs::write(d.join("pubspec.yaml"), "dependencies:\n  shared_preferences: ^2.0.0\n").unwrap();
        assert!(run_check(&d, None, &rx("pubspec.yaml", "shared_preferences")), "a named non-Dart file is read as well");
        assert!(!run_check(&d, None, &Check::Absent { path: "lib".into(), pattern: "\\.score\\b".into() }), "a leftover .score is found");
        assert!(run_check(&d, None, &Check::Absent { path: "lib/features".into(), pattern: "\\.score\\b".into() }));
        assert!(!run_check(&d, None, &Check::FlutterTest { path: "test/x.dart".into() }), "no flutter: not passed");
    }

    fn r(task: &str, class: &str, alias: &str, ok: bool, clean: bool, cost: f64, tokens: i64) -> RunResult {
        RunResult { task: task.into(), expect: class.into(), class: class.into(), alias: alias.into(), tier_of_class: String::new(), kept: ok, checks_ok: ok, clean, repairs: 0, cost_usd: cost, tokens, seconds: 10.0 }
    }

    #[test]
    fn the_report_groups_by_class_and_model_and_compares_auto_with_fixed_choices() {
        let cfg = EngineConfig::embedded();
        let rs = vec![
            r("a", "trivial", "haiku", true, true, 0.02, 10_000),
            r("a", "trivial", "sonnet", true, true, 0.05, 12_000),
            r("a", "trivial", "opus", true, true, 0.20, 12_000),
            r("b", "complex", "haiku", false, false, 0.30, 90_000),
            r("b", "complex", "sonnet", true, false, 0.40, 80_000),
            r("b", "complex", "opus", true, true, 0.90, 70_000),
        ];
        let m = by_class_and_model(&rs);
        assert_eq!(m[&("trivial".into(), "haiku".into())].success, 1);
        assert_eq!(m[&("complex".into(), "haiku".into())].success, 0);
        let v = auto_vs_baselines(&rs, &cfg);
        assert!((v["auto"].cost - (0.02 + 0.90)).abs() < 1e-9, "trivial -> haiku, complex -> opus");
        assert!((v["always-strongest"].cost - 1.10).abs() < 1e-9);
        assert!((v["always-lightest"].cost - 0.32).abs() < 1e-9);
        assert_eq!((v["auto"].success, v["always-lightest"].success), (2, 1), "the lightest model fails the complex task");
        let text = report(&rs, &cfg);
        assert!(text.contains("| trivial | haiku | 1 | 100% |") && text.contains("always-strongest") && text.contains("| b | complex | complex |"));
    }

    /// Echte evaluatie: kost echte tokens en tijd. Zie de moduledocumentatie.
    #[test]
    #[ignore]
    fn real_eval() {
        use crate::chat::{ChatCtx, ChatManager};
        use crate::db::Db;
        use crate::engine::claude::ClaudeEngine;
        use crate::engine::RunHandle;
        use crate::permissions::Permissions;
        use crate::projects::{create_project, CreateRequest, Tools};
        use std::sync::Arc;
        use std::time::{Duration, Instant};

        let flutter = crate::proc::find("flutter").expect("flutter on PATH");
        let git = crate::proc::find("git").expect("git");
        let root = std::path::PathBuf::from(std::env::var("STUDIO_EVAL_DIR").unwrap_or_else(|_| "/tmp/studio-eval".into()));
        std::fs::create_dir_all(&root).unwrap();
        let out_path = std::env::var("STUDIO_EVAL_OUT").map(std::path::PathBuf::from).unwrap_or_else(|_| root.join("results.jsonl"));
        let aliases: Vec<String> = std::env::var("STUDIO_EVAL_ALIASES").unwrap_or_else(|_| "haiku,sonnet,opus".into()).split(',').map(|s| s.trim().to_string()).collect();
        let only: Option<Vec<String>> = std::env::var("STUDIO_EVAL_TASKS").ok().filter(|v| v != "all").map(|v| v.split(',').map(|s| s.trim().to_string()).collect());

        let db = Arc::new(Db::open_in_memory().unwrap());
        let projects = root.join("projects");
        let dir = projects.join("eval_demo");
        let project = if dir.join("pubspec.yaml").exists() {
            db.project_insert(&crate::db::NewProject { name: "Eval Demo".into(), slug: "eval_demo".into(), path: dir.to_string_lossy().into(), template: "scoreboard".into(), description: "eval".into(), bundle_id: "com.example.evaldemo".into(), initial_commit: None, profile_version: crate::profile::CURRENT }).unwrap()
        } else {
            let req = CreateRequest { name: "Eval Demo".into(), template: "scoreboard".into(), description: "Scores for an evaluation".into(), org: "com.example".into() };
            create_project(&db, &Tools { flutter: flutter.clone(), git: git.clone() }, &projects, &req, &|_| {}).unwrap()
        };
        let dir = std::path::PathBuf::from(&project.path);
        let g = |args: &[&str]| crate::proc::run(&git, args, Some(&dir), Duration::from_secs(60)).unwrap();
        g(&["config", "user.email", "eval@example.com"]);
        g(&["config", "user.name", "Studio eval"]);
        // De schone beginstaat: wat er nu is, nadat alles is vastgelegd.
        g(&["add", "-A"]);
        let _ = g(&["commit", "-qm", "eval base"]);
        let base = crate::gitops::head(&dir).unwrap();
        let pkg = std::fs::read_to_string(dir.join("pubspec.yaml")).unwrap().lines().find_map(|l| l.strip_prefix("name:").map(|n| n.trim().to_string())).unwrap();

        db.setting_set("dev_mode", "true").unwrap();
        let perms = Arc::new(Permissions::new(Arc::new(|_| {}), Duration::from_secs(5)));
        let scopes: Arc<crate::scope::ScopeRegistry> = Default::default();
        let mcp = crate::mcp::start(crate::mcp::Deps { db: db.clone(), perms, preview: Arc::new(crate::preview::PreviewManager::new(Arc::new(|_, _| {}))), work_dir: root.join("shots"), scopes: scopes.clone(), vault: crate::vault::global(), emit: Arc::new(|_, _| {}) }).unwrap();
        let ctx = ChatCtx { db: db.clone(), mcp, config_dir: root.join("mcp"), scopes, checks: true, flutter: Some(flutter.clone()), emit: Arc::new(|_, _| {}) };
        let mgr = ChatManager::new(Arc::new(ClaudeEngine::new()));
        let cfg = EngineConfig::embedded();

        let done: Vec<RunResult> = std::fs::read_to_string(&out_path).unwrap_or_default().lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
        let mut results = done.clone();
        for task in load_tasks().tasks.iter().filter(|t| only.as_ref().is_none_or(|o| o.contains(&t.id))) {
            for alias in &aliases {
                if done.iter().any(|d| d.task == task.id && &d.alias == alias) {
                    println!("skip {} {alias} (already measured)", task.id);
                    continue;
                }
                // Terug naar de schone staat, dan de voorbereiding van de taak (bijvoorbeeld een ingebouwde bug) vastleggen.
                g(&["reset", "-q", "--hard", &base]);
                g(&["clean", "-fdq"]);
                for s in &task.setup {
                    let p = dir.join(&s.file);
                    let t = std::fs::read_to_string(&p).unwrap();
                    std::fs::write(&p, t.replacen(&s.replace[0], &s.replace[1], 1)).unwrap();
                }
                for h in &task.hidden_tests {
                    let p = dir.join(&h.path);
                    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
                    std::fs::write(p, h.content.replace("__PKG__", &pkg)).unwrap();
                }
                g(&["add", "-A"]);
                let _ = g(&["commit", "-qm", "eval setup"]);

                // Elke run begint met een schone sessie: anders onthoudt het model eerdere taken en zijn de runs niet onafhankelijk.
                db.session_create(&project.id, "claude-code").unwrap();
                db.setting_set("model_override", alias).unwrap();
                let handle = Arc::new(RunHandle::default());
                let finished = Arc::new(std::sync::atomic::AtomicBool::new(false));
                {
                    let (h, f) = (handle.clone(), finished.clone());
                    std::thread::spawn(move || {
                        let t = Instant::now();
                        while t.elapsed() < Duration::from_secs(1200) && !f.load(std::sync::atomic::Ordering::SeqCst) {
                            std::thread::sleep(Duration::from_secs(2));
                        }
                        if !f.load(std::sync::atomic::Ordering::SeqCst) {
                            h.cancel();
                        }
                    });
                }
                let started = Instant::now();
                mgr.run_flow(&ctx, &project.id, &task.prompt, None, "en", &handle);
                finished.store(true, std::sync::atomic::Ordering::SeqCst);
                let secs = started.elapsed().as_secs_f64();
                let row = db.runs_summary(&project.id, 1).unwrap().last.expect("a run was recorded");
                let verdicts: Vec<(String, bool)> = task.checks.iter().map(|c| (format!("{c:?}"), run_check(&dir, Some(&flutter), c))).collect();
                for (c, ok) in verdicts.iter().filter(|(_, ok)| !ok) {
                    println!("  failed check ({ok}): {c}");
                }
                let checks_ok = verdicts.iter().all(|(_, ok)| *ok);
                let kept = row.ok && !row.rolled_back;
                let res = RunResult {
                    task: task.id.clone(),
                    expect: task.expect.clone(),
                    class: row.class.clone(),
                    alias: alias.clone(),
                    tier_of_class: cfg.class_map.get(&row.class).cloned().unwrap_or_default(),
                    kept,
                    checks_ok,
                    clean: row.is_clean(),
                    repairs: row.repairs,
                    cost_usd: row.cost_usd,
                    tokens: row.tokens,
                    seconds: secs,
                };
                println!("{} {alias}: class={} kept={kept} checks={checks_ok} repairs={} cost=${:.3} tokens={} {:.0}s", task.id, res.class, res.repairs, res.cost_usd, res.tokens, secs);
                use std::io::Write;
                let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&out_path).unwrap();
                writeln!(f, "{}", serde_json::to_string(&res).unwrap()).unwrap();
                results.push(res);
            }
        }
        g(&["reset", "-q", "--hard", &base]);
        let text = report(&results, &cfg);
        std::fs::write(root.join("report.md"), &text).unwrap();
        println!("\n{text}");
    }
}

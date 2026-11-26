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

/// `basic` = de eerste dertien opdrachten op het kleine scoreboard-project; `hard` = de zwaardere set op het grotere project.
pub fn load_tasks_of(set: &str) -> TaskFile {
    let file = if set == "hard" { "hard-tasks.json" } else { "tasks.json" };
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../evals").join(file);
    serde_json::from_str(&std::fs::read_to_string(p).expect("the task file")).expect("a valid task file")
}

pub fn load_tasks() -> TaskFile {
    load_tasks_of("basic")
}

/// Het grotere testproject (Trip Expenses) dat over het sjabloon heen wordt gezet.
pub fn fixture_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../evals/project")
}

fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let target = to.join(e.file_name());
        if e.path().is_dir() {
            copy_tree(&e.path(), &target)?;
        } else {
            std::fs::copy(e.path(), target)?;
        }
    }
    Ok(())
}

/// Vervangt de code van een nieuw project door het grotere testproject (pubspec, profiel en thema blijven).
pub fn install_fixture(dir: &Path) -> std::io::Result<()> {
    for rel in ["lib/features", "lib/core/models", "lib/core/navigation", "test"] {
        let _ = std::fs::remove_dir_all(dir.join(rel));
    }
    copy_tree(&fixture_dir(), dir)
}

const BASELINE_GUIDE: &str = include_str!("../../evals/baseline-claude.md");

/// De basislijn: geen projectprofiel (geen skills, regels, hooks, schermindex of per-feature regels) maar één lange CLAUDE.md met
/// algemene richtlijnen, de bestandslijst en de volledige code, zoals een gebruiker zonder Studio die zelf zou schrijven.
pub fn make_baseline(dir: &Path) -> std::io::Result<()> {
    for rel in [".claude", ".studio"] {
        let _ = std::fs::remove_dir_all(dir.join(rel));
    }
    fn strip(d: &Path) {
        let Ok(rd) = std::fs::read_dir(d) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                strip(&p);
            } else if p.file_name().is_some_and(|n| n == "CLAUDE.md") {
                let _ = std::fs::remove_file(&p);
            }
        }
    }
    strip(&dir.join("lib"));
    let mut files = vec![];
    dart_files(&dir.join("lib"), &mut files);
    files.sort();
    let mut doc = String::from(BASELINE_GUIDE);
    doc.push_str("\n\n## Project files\n\n");
    for f in &files {
        doc.push_str(&format!("- {}\n", f.strip_prefix(dir).unwrap_or(f).to_string_lossy().replace('\\', "/")));
    }
    doc.push_str("\n## Current source code\n");
    for f in &files {
        let rel = f.strip_prefix(dir).unwrap_or(f).to_string_lossy().replace('\\', "/");
        doc.push_str(&format!("\n### {rel}\n\n```dart\n{}```\n", std::fs::read_to_string(f).unwrap_or_default()));
    }
    std::fs::write(dir.join("CLAUDE.md"), doc)
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
    run_check_verbose(dir, flutter, c).0
}

/// Als `run_check`, met bij een mislukte test de laatste regels uitvoer (om te zien of de taak of de test faalde).
pub fn run_check_verbose(dir: &Path, flutter: Option<&Path>, c: &Check) -> (bool, String) {
    let re = |p: &str| regex_lite::Regex::new(p).expect("valid check pattern");
    let plain = |b: bool| (b, String::new());
    match c {
        Check::Regex { path, pattern } => plain(re(pattern).is_match(&text_of(dir, path))),
        Check::Absent { path, pattern } => plain(!re(pattern).is_match(&text_of(dir, path))),
        Check::Exists { path } => plain(dir.join(path).exists()),
        Check::PathMatches { path, pattern } => plain(paths_under(dir, path).iter().any(|p| re(pattern).is_match(p))),
        Check::FlutterTest { path } => match flutter.and_then(|f| crate::proc::run(f, &["test", path], Some(dir), std::time::Duration::from_secs(600)).ok()) {
            Some(o) if o.ok() => (true, String::new()),
            Some(o) => {
                let text = o.combined();
                let lines: Vec<&str> = text.lines().collect();
                (false, lines[lines.len().saturating_sub(45)..].join("\n"))
            }
            None => (false, "flutter could not run".into()),
        },
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
    /// router (volledige Studio) of baseline (zonder scope-router en projectprofiel), en welke set.
    #[serde(default)]
    pub variant: String,
    #[serde(default)]
    pub set: String,
    #[serde(default)]
    pub input_tokens: i64,
    #[serde(default)]
    pub output_tokens: i64,
    #[serde(default)]
    pub cache_read_tokens: i64,
    #[serde(default)]
    pub cache_write_tokens: i64,
}

impl RunResult {
    /// Tokens die echt verwerkt moeten worden: invoer, uitvoer en naar de cache geschreven (gelezen cache is bijna gratis).
    pub fn fresh_tokens(&self) -> i64 {
        self.input_tokens + self.output_tokens + self.cache_write_tokens
    }

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
    pub input: i64,
    pub output: i64,
    pub cache_read: i64,
    pub cache_write: i64,
}

impl Cell {
    fn add(&mut self, r: &RunResult) {
        self.runs += 1;
        self.success += r.success() as usize;
        self.clean += (r.success() && r.clean) as usize;
        self.cost += r.cost_usd;
        self.tokens += r.tokens;
        self.seconds += r.seconds;
        self.input += r.input_tokens;
        self.output += r.output_tokens;
        self.cache_read += r.cache_read_tokens;
        self.cache_write += r.cache_write_tokens;
    }
}

/// Eén overzicht: per klasse, variant en model het slagingspercentage, de ruwe tokens, kosten en tijd (gemiddelden per run), plus
/// de vergelijking tussen de varianten per model. `class_of` geeft per taak de klasse die Studio zelf koos (de basislijn kent die
/// niet, want zonder router wordt er niet geclassificeerd).
pub fn overview(results: &[RunResult], class_of: &BTreeMap<String, String>) -> String {
    let class = |r: &RunResult| if r.variant == "baseline" { class_of.get(&r.task).cloned().unwrap_or_else(|| r.expect.clone()) } else { r.class.clone() };
    let mut groups: BTreeMap<(String, String, String), Cell> = BTreeMap::new();
    for r in results {
        groups.entry((class(r), r.variant.clone(), r.alias.clone())).or_default().add(r);
    }
    let k = |v: i64, n: f64| format!("{:.0}k", v as f64 / n / 1000.0);
    let mut s = String::from("## Per class, variant and model (averages per run)\n\n| class | variant | model | runs | success | clean first time | input | output | cache read | cache write | cost | time |\n|---|---|---|---|---|---|---|---|---|---|---|---|\n");
    for ((cls, var, alias), c) in &groups {
        let n = c.runs as f64;
        s.push_str(&format!(
            "| {cls} | {var} | {alias} | {} | {:.0}% | {:.0}% | {} | {} | {} | {} | ${:.3} | {:.0}s |\n",
            c.runs,
            100.0 * c.success as f64 / n,
            100.0 * c.clean as f64 / n,
            k(c.input, n),
            k(c.output, n),
            k(c.cache_read, n),
            k(c.cache_write, n),
            c.cost / n,
            c.seconds / n
        ));
    }
    s.push_str("\n## Router and profile versus baseline, per model (same tasks)\n\n| model | variant | runs | success | fresh tokens (input+output+cache write) | all tokens | cost | time |\n|---|---|---|---|---|---|---|---|\n");
    let mut aliases: Vec<&str> = results.iter().map(|r| r.alias.as_str()).collect();
    aliases.sort();
    aliases.dedup();
    for a in aliases {
        for var in ["router", "baseline"] {
            let rs: Vec<&RunResult> = results.iter().filter(|r| r.alias == a && r.variant == var).collect();
            if rs.is_empty() {
                continue;
            }
            let n = rs.len() as f64;
            let (succ, fresh, all, cost, secs) = rs.iter().fold((0usize, 0i64, 0i64, 0f64, 0f64), |t, r| (t.0 + r.success() as usize, t.1 + r.fresh_tokens(), t.2 + r.input_tokens + r.output_tokens + r.cache_read_tokens + r.cache_write_tokens, t.3 + r.cost_usd, t.4 + r.seconds));
            s.push_str(&format!("| {a} | {var} | {} | {:.0}% | {} | {} | ${:.2} | {:.0}s |\n", rs.len(), 100.0 * succ as f64 / n, k(fresh, n), k(all, n), cost / n, secs / n));
        }
    }
    s.push_str("\n## Per task\n\n| task | class | variant | haiku | sonnet | opus |\n|---|---|---|---|---|---|\n");
    let mut tasks: Vec<&str> = results.iter().map(|r| r.task.as_str()).collect();
    tasks.sort();
    tasks.dedup();
    for t in tasks {
        for var in ["router", "baseline"] {
            let cells: Vec<String> = ["haiku", "sonnet", "opus"]
                .iter()
                .map(|a| results.iter().find(|r| r.task == t && r.variant == var && r.alias == *a).map(|r| format!("{} ${:.2} {:.0}k", if r.success() { "ok" } else { "FAIL" }, r.cost_usd, r.fresh_tokens() as f64 / 1000.0)).unwrap_or_else(|| "–".into()))
                .collect();
            if cells.iter().all(|c| c == "–") {
                continue;
            }
            s.push_str(&format!("| {t} | {} | {var} | {} |\n", class_of.get(t).cloned().unwrap_or_default(), cells.join(" | ")));
        }
    }
    s
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
        let hard = load_tasks_of("hard");
        assert_eq!(hard.tasks.len(), 9);
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
        RunResult { task: task.into(), expect: class.into(), class: class.into(), alias: alias.into(), tier_of_class: String::new(), kept: ok, checks_ok: ok, clean, repairs: 0, cost_usd: cost, tokens, seconds: 10.0, variant: "router".into(), set: "basic".into(), input_tokens: 0, output_tokens: 0, cache_read_tokens: 0, cache_write_tokens: 0 }
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

    #[test]
    fn the_hard_tasks_apply_to_the_fixture_project_and_each_has_a_behaviour_test() {
        let hard = load_tasks_of("hard");
        let mut ids: Vec<&str> = hard.tasks.iter().map(|t| t.id.as_str()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 9);
        for t in &hard.tasks {
            assert!(!t.hidden_tests.is_empty(), "{} has a hidden behaviour test", t.id);
            assert!(t.hidden_tests.iter().all(|h| h.path.starts_with("test/") && h.content.contains("__PKG__") && h.content.contains("expect(")), "{}", t.id);
            assert!(t.checks.iter().any(|c| matches!(c, Check::FlutterTest { .. })), "{} runs a real test", t.id);
            for s in &t.setup {
                let text = std::fs::read_to_string(fixture_dir().join(&s.file)).unwrap_or_else(|_| panic!("{}: {}", t.id, s.file));
                assert!(text.contains(&s.replace[0]), "{}: setup text exists in the fixture", t.id);
            }
        }
        // De hidden tests komen niet in het project voordat de run klaar is: ze staan niet in de fixture zelf.
        assert!(!fixture_dir().join("test").read_dir().unwrap().flatten().any(|e| e.file_name().to_string_lossy().starts_with("eval_")));
    }

    #[test]
    fn the_baseline_has_no_profile_but_one_long_claude_md_with_the_whole_code() {
        let d = std::env::temp_dir().join(format!("studio-baseline-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(d.join(".claude/skills/studio-screen")).unwrap();
        std::fs::create_dir_all(d.join(".studio")).unwrap();
        std::fs::create_dir_all(d.join("lib/features/x")).unwrap();
        std::fs::write(d.join(".claude/settings.json"), "{}").unwrap();
        std::fs::write(d.join("CLAUDE.md"), "short profile").unwrap();
        std::fs::write(d.join("lib/features/x/CLAUDE.md"), "local rules").unwrap();
        std::fs::write(d.join("lib/features/x/x_screen.dart"), "class XScreen {}\n").unwrap();
        make_baseline(&d).unwrap();
        assert!(!d.join(".claude").exists() && !d.join(".studio").exists() && !d.join("lib/features/x/CLAUDE.md").exists());
        let md = std::fs::read_to_string(d.join("CLAUDE.md")).unwrap();
        assert!(md.lines().count() > 80, "long: {} lines", md.lines().count());
        assert!(md.contains("lib/features/x/x_screen.dart") && md.contains("class XScreen {}") && md.contains("## General rules"));
        assert!(!md.contains("short profile") && !md.contains("breid_bereik_uit") && !md.contains("scherm_index"), "no Studio-specific guidance");
    }

    #[test]
    fn installing_the_fixture_replaces_the_template_code_and_keeps_the_rest() {
        let d = std::env::temp_dir().join(format!("studio-fixture-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(d.join("lib/features/scoreboard")).unwrap();
        std::fs::create_dir_all(d.join("lib/core/theme")).unwrap();
        std::fs::create_dir_all(d.join("test")).unwrap();
        std::fs::write(d.join("lib/core/theme/app_theme.dart"), "theme").unwrap();
        std::fs::write(d.join("test/widget_test.dart"), "old").unwrap();
        std::fs::write(d.join("pubspec.yaml"), "name: eval_demo\n").unwrap();
        install_fixture(&d).unwrap();
        assert!(!d.join("lib/features/scoreboard").exists());
        assert!(d.join("lib/features/trips/trips_screen.dart").exists() && d.join("lib/core/data/trips_controller.dart").exists());
        assert_eq!(std::fs::read_to_string(d.join("lib/core/theme/app_theme.dart")).unwrap(), "theme", "the generated theme stays");
        assert!(std::fs::read_to_string(d.join("test/widget_test.dart")).unwrap().contains("Lisbon"));
        assert_eq!(std::fs::read_to_string(d.join("pubspec.yaml")).unwrap(), "name: eval_demo\n");
    }

    #[test]
    fn the_overview_compares_variants_per_model_and_uses_the_routers_class_for_the_baseline() {
        let mk = |task: &str, variant: &str, alias: &str, ok: bool, cost: f64, i: i64, o: i64, cr: i64, cw: i64| RunResult {
            task: task.into(), expect: "complex".into(), class: if variant == "baseline" { "complex".into() } else { "normal".into() }, alias: alias.into(), tier_of_class: String::new(), kept: ok, checks_ok: ok, clean: ok, repairs: 0,
            cost_usd: cost, tokens: i + o + cr + cw, seconds: 30.0, variant: variant.into(), set: "hard".into(), input_tokens: i, output_tokens: o, cache_read_tokens: cr, cache_write_tokens: cw,
        };
        let rs = vec![
            mk("a", "router", "haiku", true, 0.02, 1_000, 3_000, 100_000, 20_000),
            mk("a", "baseline", "haiku", true, 0.05, 2_000, 4_000, 300_000, 60_000),
            mk("a", "router", "opus", true, 0.40, 1_000, 5_000, 120_000, 20_000),
            mk("a", "baseline", "opus", false, 0.90, 2_000, 6_000, 400_000, 70_000),
        ];
        let class_of: BTreeMap<String, String> = [("a".to_string(), "normal".to_string())].into();
        let text = overview(&rs, &class_of);
        assert!(text.contains("| normal | baseline | haiku | 1 | 100% |"), "the baseline row is listed under the class the router gave the task");
        assert!(text.contains("| normal | router | opus | 1 | 100% | 100% | 1k | 5k | 120k | 20k | $0.400 | 30s |"), "{text}");
        assert!(text.contains("| opus | baseline | 1 | 0% | 78k |"), "fresh tokens = input + output + cache write: {text}");
        assert!(text.contains("| haiku | router | 1 | 100% | 24k |"));
        assert_eq!(rs[0].fresh_tokens(), 24_000);
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
        let set = std::env::var("STUDIO_EVAL_SET").unwrap_or_else(|_| "basic".into());
        let variant = std::env::var("STUDIO_EVAL_VARIANT").unwrap_or_else(|_| "router".into());
        assert!(matches!(variant.as_str(), "router" | "baseline"));
        let default_out = if set == "basic" && variant == "router" { "results.jsonl".to_string() } else { format!("results-{set}-{variant}.jsonl") };
        let out_path = std::env::var("STUDIO_EVAL_OUT").map(std::path::PathBuf::from).unwrap_or_else(|_| root.join(default_out));
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
        // De grotere set draait op het grotere testproject; de basislijn haalt daar het projectprofiel weer vanaf.
        if set == "hard" && !dir.join("lib/core/data/trips_controller.dart").exists() {
            install_fixture(&dir).unwrap();
            let a = crate::proc::run(&flutter, &["analyze"], Some(&dir), Duration::from_secs(300)).unwrap();
            assert!(a.ok(), "the fixture project must analyze clean: {}", a.combined());
            let t = crate::proc::run(&flutter, &["test"], Some(&dir), Duration::from_secs(300)).unwrap();
            assert!(t.ok(), "the fixture project's own tests must pass: {}", t.combined());
        }
        if variant == "baseline" && dir.join(".claude").exists() {
            make_baseline(&dir).unwrap();
        }
        // De schone beginstaat: wat er nu is, nadat alles is vastgelegd.
        g(&["add", "-A"]);
        let _ = g(&["commit", "-qm", "eval base"]);
        let base = crate::gitops::head(&dir).unwrap();
        let pkg = std::fs::read_to_string(dir.join("pubspec.yaml")).unwrap().lines().find_map(|l| l.strip_prefix("name:").map(|n| n.trim().to_string())).unwrap();

        db.setting_set("dev_mode", "true").unwrap();
        let perms = Arc::new(Permissions::new(Arc::new(|_| {}), Duration::from_secs(5)));
        let scopes: Arc<crate::scope::ScopeRegistry> = Default::default();
        let mcp = crate::mcp::start(crate::mcp::Deps { db: db.clone(), perms, preview: Arc::new(crate::preview::PreviewManager::new(Arc::new(|_, _| {}))), work_dir: root.join("shots"), scopes: scopes.clone(), vault: crate::vault::global(), emit: Arc::new(|_, _| {}) }).unwrap();
        let ablation = if variant == "baseline" { crate::chat::Ablation { no_router: true, no_profile: true } } else { Default::default() };
        let ctx = ChatCtx { db: db.clone(), mcp, config_dir: root.join("mcp"), scopes, checks: true, flutter: Some(flutter.clone()), ablation, emit: Arc::new(|_, _| {}) };
        // Een andere AI meten: STUDIO_EVAL_ENGINE=codex of gemini (vereist een geïnstalleerde en ingelogde CLI; modelnamen worden dan genegeerd,
        // gebruik STUDIO_EVAL_ALIASES=default). Het resultaat landt in dezelfde `runs`-tabel met de naam van de AI erbij.
        if let Ok(id) = std::env::var("STUDIO_EVAL_ENGINE") {
            crate::engine::registry::set_order(&db, &[id]).unwrap();
        }
        let mgr = ChatManager::new(Arc::new(ClaudeEngine::new()));
        let cfg = EngineConfig::embedded();

        let done: Vec<RunResult> = std::fs::read_to_string(&out_path).unwrap_or_default().lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
        let mut results = done.clone();
        for task in load_tasks_of(&set).tasks.iter().filter(|t| only.as_ref().is_none_or(|o| o.contains(&t.id))) {
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
                // Taken met een verborgen test (alleen in de basisset die ze vooraf zette) blijven zoals ze waren.
                if set == "basic" {
                    for h in &task.hidden_tests {
                        let p = dir.join(&h.path);
                        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
                        std::fs::write(p, h.content.replace("__PKG__", &pkg)).unwrap();
                    }
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
                // Geen enkel antwoord (bijvoorbeeld een bereikte gebruikslimiet): niet als mislukking vastleggen maar stoppen, zodat de
                // run later hervat kan worden zonder dat er een verkeerd cijfer in de resultaten staat.
                assert!(row.cost_usd > 0.0 || row.tokens > 0, "{} {alias}: the AI did not answer (usage limit reached?); stopping so the run can be resumed", task.id);
                // De verborgen tests komen pas nu in het project: het model heeft ze nooit gezien (ook niet via de controle of reparatie).
                if set == "hard" {
                    for h in &task.hidden_tests {
                        let p = dir.join(&h.path);
                        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
                        std::fs::write(p, h.content.replace("__PKG__", &pkg)).unwrap();
                    }
                }
                let mut details = vec![];
                let verdicts: Vec<(String, bool)> = task
                    .checks
                    .iter()
                    .map(|c| {
                        let (ok, detail) = run_check_verbose(&dir, Some(&flutter), c);
                        details.push(detail);
                        (format!("{c:?}"), ok)
                    })
                    .collect();
                for ((c, _), d) in verdicts.iter().zip(&details).filter(|((_, ok), _)| !ok) {
                    println!("  failed check: {c}\n{d}");
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
                    variant: variant.clone(),
                    set: set.clone(),
                    input_tokens: row.input_tokens,
                    output_tokens: row.output_tokens,
                    cache_read_tokens: row.cache_read_tokens,
                    cache_write_tokens: row.cache_write_tokens,
                };
                println!("{} {variant} {alias}: class={} kept={kept} checks={checks_ok} repairs={} cost=${:.3} tokens={} (in {} out {} cache r {} w {}) {:.0}s", task.id, res.class, res.repairs, res.cost_usd, res.tokens, res.input_tokens, res.output_tokens, res.cache_read_tokens, res.cache_write_tokens, secs);
                use std::io::Write;
                let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&out_path).unwrap();
                writeln!(f, "{}", serde_json::to_string(&res).unwrap()).unwrap();
                results.push(res);
            }
        }
        g(&["reset", "-q", "--hard", &base]);
        let text = report(&results, &cfg);
        std::fs::write(root.join(format!("report-{set}-{variant}.md")), &text).unwrap();
        println!("\n{text}");
    }

    /// Voegt de resultaatbestanden (komma-gescheiden in `STUDIO_EVAL_FILES`) samen tot één overzicht en schrijft het naar
    /// `STUDIO_EVAL_REPORT` (standaard evals/last-report-hard.md).
    #[test]
    #[ignore]
    fn real_eval_overview() {
        let files = std::env::var("STUDIO_EVAL_FILES").expect("STUDIO_EVAL_FILES");
        let mut results: Vec<RunResult> = vec![];
        for f in files.split(',') {
            results.extend(std::fs::read_to_string(f.trim()).unwrap_or_default().lines().filter_map(|l| serde_json::from_str::<RunResult>(l).ok()));
        }
        let class_of: BTreeMap<String, String> = results.iter().filter(|r| r.variant != "baseline").fold(BTreeMap::new(), |mut m, r| {
            m.entry(r.task.clone()).or_insert_with(|| r.class.clone());
            m
        });
        let text = overview(&results, &class_of);
        let out = std::env::var("STUDIO_EVAL_REPORT").unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/../evals/last-report-hard.md").into());
        std::fs::write(out, &text).unwrap();
        println!("{text}");
    }
}

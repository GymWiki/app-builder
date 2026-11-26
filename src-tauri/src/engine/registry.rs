//! Welke AI's Studio kent (`engines/registry.json`), of ze op deze computer staan en ingelogd zijn, welke de limiet bereikt heeft,
//! en welke er voor de volgende opdracht gebruikt wordt. Studio start alleen de officiële CLI's als subproces: inloggen doet de
//! gebruiker zelf in de CLI en Studio leest nooit tokens of sessies (alleen of een bestand er is, nooit wat erin staat).

use crate::db::Db;
use crate::error::AppResult;
use crate::proc;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

const REGISTRY: &str = include_str!("../../../engines/registry.json");

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EngineSpec {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub binary: String,
    /// Lang in gebruik en bewezen? Anders krijgt elk resultaat een extra controle (analyse én tests).
    pub proven: bool,
    /// Argumenten om in te loggen (`<binary> <args>`); leeg = de CLI vraagt er zelf om bij de eerste start.
    pub login_args: Vec<String>,
    pub install_hint: String,
    pub docs: String,
}

pub fn specs() -> Vec<EngineSpec> {
    #[derive(Deserialize)]
    struct File {
        engines: Vec<EngineSpec>,
    }
    serde_json::from_str::<File>(REGISTRY).map(|f| f.engines).unwrap_or_default()
}

pub fn spec(id: &str) -> Option<EngineSpec> {
    specs().into_iter().find(|s| s.id == id)
}

pub const DEFAULT_ENGINE: &str = "claude-code";

// ───────────── herkennen van problemen in uitvoer ─────────────

/// De AI meldt dat het gebruik op is. `reset` is de tijd uit de melding, als die er staat ("try again at 3:41 PM", "reset after 3h5m2s").
#[derive(Debug, Clone, PartialEq)]
pub struct Limit {
    pub reset: Option<String>,
}

/// Herkent een gebruikslimiet in de uitvoer van Claude Code, Codex of Gemini. Alleen duidelijke meldingen: een gewone fout is geen limiet.
pub fn detect_limit(text: &str) -> Option<Limit> {
    let t = text.to_lowercase();
    const SIGNS: &[&str] = &[
        "hit your usage limit",
        "usage limit reached",
        "claude ai usage limit reached",
        "5-hour limit reached",
        "exhausted your capacity",
        "quota exceeded",
        "resource_exhausted",
        "you've reached your",
        "rate limit exceeded",
        "too many requests",
    ];
    if !SIGNS.iter().any(|s| t.contains(s)) {
        return None;
    }
    let reset = t
        .find("try again at ")
        .map(|i| &text[i + 13..])
        .or_else(|| t.find("reset after ").map(|i| &text[i + 12..]))
        .or_else(|| t.find("resets at ").map(|i| &text[i + 10..]))
        .map(|r| r.split(['.', '\n', '"']).next().unwrap_or("").trim().to_string())
        .filter(|r| !r.is_empty() && r.len() < 40);
    Some(Limit { reset })
}

/// De CLI vraagt om in te loggen of een aanmeldmethode te kiezen.
pub fn detect_login_problem(text: &str) -> bool {
    let t = text.to_lowercase();
    ["not logged in", "please run /login", "please set an auth method", "authentication required", "please log in", "invalid api key", "codex login", "run `codex login`"].iter().any(|s| t.contains(s))
}

// ───────────── toestand ─────────────

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EngineStatus {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub installed: bool,
    pub version: Option<String>,
    /// `None` = onbekend (bv. Gemini met een eigen sleutel in de omgeving of nog niet gekozen).
    pub logged_in: Option<bool>,
    pub proven: bool,
    pub install_hint: String,
    pub docs: String,
    /// Tot wanneer (unix-seconden) deze AI op "limiet bereikt" staat.
    pub limited_until: Option<i64>,
}

impl EngineStatus {
    /// Kan deze AI nu een opdracht krijgen? Ingelogd, of (nog) onbekend.
    pub fn ready(&self) -> bool {
        self.installed && self.logged_in != Some(false)
    }
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from)
}

/// Gemini: een eigen sleutel of Vertex in de omgeving, of een inloggegevensbestand dat bestaat. We kijken alleen of het er is.
fn gemini_logged_in() -> Option<bool> {
    let env = ["GEMINI_API_KEY", "GOOGLE_API_KEY", "GOOGLE_GENAI_USE_VERTEXAI", "GOOGLE_GENAI_USE_GCA"].iter().any(|k| std::env::var_os(k).is_some());
    let file = home().map(|h| h.join(".gemini/oauth_creds.json").is_file()).unwrap_or(false);
    (env || file).then_some(true)
}

pub fn detect(spec: &EngineSpec) -> EngineStatus {
    let program = proc::find(&spec.binary);
    let version = program.as_ref().and_then(|p| proc::run(p, &["--version"], None, Duration::from_secs(20)).ok()).map(|o| o.combined().lines().next().unwrap_or("").trim().to_string()).filter(|v| !v.is_empty());
    let logged_in = match (&program, spec.id.as_str()) {
        (None, _) => Some(false),
        (Some(p), "claude-code") => proc::run(p, &["auth", "status", "--json"], None, Duration::from_secs(20)).ok().map(|o| {
            serde_json::from_str::<serde_json::Value>(o.stdout.trim()).ok().and_then(|v| v["loggedIn"].as_bool()).unwrap_or_else(|| o.ok())
        }),
        (Some(p), "codex") => proc::run(p, &["login", "status"], None, Duration::from_secs(20)).ok().map(|o| o.ok() && !o.combined().to_lowercase().contains("not logged in")),
        (Some(_), "gemini") => gemini_logged_in(),
        _ => None,
    };
    EngineStatus { id: spec.id.clone(), name: spec.name.clone(), provider: spec.provider.clone(), installed: program.is_some(), version, logged_in, proven: spec.proven, install_hint: spec.install_hint.clone(), docs: spec.docs.clone(), limited_until: None }
}

// ───────────── volgorde en limieten (in de instellingen) ─────────────

const ORDER_KEY: &str = "engines.order";

pub fn order(db: &Db) -> Vec<String> {
    let known: Vec<String> = specs().into_iter().map(|s| s.id).collect();
    let saved = db.setting_get(ORDER_KEY).ok().flatten().unwrap_or_default();
    let mut out: Vec<String> = saved.split(',').map(str::trim).filter(|s| known.iter().any(|k| k == s)).map(String::from).collect();
    out.dedup();
    if out.is_empty() {
        out.push(DEFAULT_ENGINE.to_string());
    }
    out
}

pub fn set_order(db: &Db, ids: &[String]) -> AppResult<()> {
    let known: Vec<String> = specs().into_iter().map(|s| s.id).collect();
    let mut clean: Vec<&str> = vec![];
    for i in ids {
        if known.contains(i) && !clean.contains(&i.as_str()) {
            clean.push(i);
        }
    }
    db.setting_set(ORDER_KEY, &clean.join(","))
}

fn limit_key(id: &str) -> String {
    format!("engine_limit:{id}")
}

/// Standaard wachttijd als de melding geen tijd noemt.
pub const DEFAULT_COOLDOWN_SECS: i64 = 2 * 3600;

pub fn mark_limited(db: &Db, id: &str, now: i64) {
    let _ = db.setting_set(&limit_key(id), &(now + DEFAULT_COOLDOWN_SECS).to_string());
}

pub fn clear_limit(db: &Db, id: &str) {
    let _ = db.setting_set(&limit_key(id), "");
}

pub fn limited_until(db: &Db, id: &str, now: i64) -> Option<i64> {
    db.setting_get(&limit_key(id)).ok().flatten().and_then(|v| v.parse::<i64>().ok()).filter(|t| *t > now)
}

pub fn now_unix() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// Kiest de AI voor een nieuwe opdracht: de eerste in de volgorde die klaar is en niet op "limiet" staat; anders de eerste die klaar is;
/// anders de eerste uit de volgorde (dan meldt de run zelf wat er mis is). Wordt alleen aan het begin van een opdracht aangeroepen:
/// een lopende opdracht wisselt nooit van AI.
pub fn pick_engine(order: &[String], ready: &dyn Fn(&str) -> bool, limited: &dyn Fn(&str) -> bool) -> String {
    order
        .iter()
        .find(|id| ready(id) && !limited(id))
        .or_else(|| order.iter().find(|id| ready(id)))
        .or_else(|| order.first())
        .cloned()
        .unwrap_or_else(|| DEFAULT_ENGINE.to_string())
}

// ───────────── overdracht ─────────────

/// Korte, engine-neutrale overdrachtsnotitie als de vorige opdracht met een andere AI is gedaan. De rest van de context staat
/// in `STUDIO.md`, de schermindex en de laatste momentopname (alles in het project, voor elke AI leesbaar).
pub fn handover_note(from: &str, to: &str, last_saved: Option<&str>, recent_requests: &[String]) -> String {
    let name = |id: &str| spec(id).map(|s| s.name).unwrap_or_else(|| id.to_string());
    let mut s = format!("Handover: this app was worked on with {} before; you are {} now. Read STUDIO.md and .studio/screens.md first, they describe the app and where things are.", name(from), name(to));
    if let Some(l) = last_saved.filter(|l| !l.trim().is_empty()) {
        s.push_str(&format!(" The last saved change was: \"{}\".", l.trim().chars().take(120).collect::<String>()));
    }
    let r: Vec<String> = recent_requests.iter().rev().take(3).rev().map(|t| format!("\"{}\"", t.replace('\n', " ").chars().take(120).collect::<String>())).collect();
    if !r.is_empty() {
        s.push_str(&format!(" Recent requests from the user: {}.", r.join("; ")));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fx(name: &str) -> String {
        std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/engines").join(name)).unwrap()
    }

    #[test]
    fn the_registry_lists_three_official_clis_and_only_claude_is_proven() {
        let s = specs();
        assert_eq!(s.iter().map(|x| x.binary.as_str()).collect::<Vec<_>>(), ["claude", "codex", "gemini"]);
        assert_eq!(s.iter().filter(|x| x.proven).count(), 1);
        assert!(spec("codex").unwrap().install_hint.contains("@openai/codex") && spec("gemini").unwrap().install_hint.contains("@google/gemini-cli"));
        assert!(spec("nope").is_none());
    }

    #[test]
    fn limits_are_recognised_in_the_output_of_each_cli_and_ordinary_errors_are_not() {
        let l = detect_limit(&fx("codex_limit.jsonl")).unwrap();
        assert_eq!(l.reset.as_deref(), Some("3:41 PM"));
        let g = detect_limit(&fx("gemini_limit.jsonl")).unwrap();
        assert!(g.reset.as_deref().is_some_and(|r| r.starts_with("3h5m2s")), "{g:?}");
        assert!(detect_limit("Claude AI usage limit reached|1795700000").is_some());
        assert!(detect_limit("Error: 5-hour limit reached ∙ resets 4pm").is_some());
        for ok in [fx("codex_ok.jsonl"), fx("gemini_ok.jsonl"), fx("codex_reconnect.jsonl"), "error: file not found".to_string(), "analysis failed: 3 issues".to_string()] {
            assert!(detect_limit(&ok).is_none(), "{ok}");
        }
    }

    #[test]
    fn a_missing_login_is_recognised() {
        assert!(detect_login_problem(&fx("gemini_not_logged_in.txt")));
        assert!(detect_login_problem("Not logged in · Please run /login") && detect_login_problem("Not logged in"));
        assert!(!detect_login_problem(&fx("codex_ok.jsonl")));
    }

    #[test]
    fn the_engine_is_chosen_at_the_start_of_a_task_by_order_readiness_and_limits() {
        let order: Vec<String> = ["claude-code", "codex", "gemini"].iter().map(|s| s.to_string()).collect();
        let all = |_: &str| true;
        let none = |_: &str| false;
        assert_eq!(pick_engine(&order, &all, &none), "claude-code");
        assert_eq!(pick_engine(&order, &all, &|i| i == "claude-code"), "codex", "limit reached: the next one in line");
        assert_eq!(pick_engine(&order, &|i| i != "codex", &|i| i == "claude-code"), "gemini", "…skipping one that is not logged in");
        assert_eq!(pick_engine(&order, &all, &|_| true), "claude-code", "everything limited: the first ready one (it will say what is wrong)");
        assert_eq!(pick_engine(&order, &none, &none), "claude-code");
        assert_eq!(pick_engine(&[], &all, &none), DEFAULT_ENGINE);
    }

    #[test]
    fn order_and_limits_are_stored_in_the_settings() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(order(&db), ["claude-code"]);
        set_order(&db, &["codex".into(), "bogus".into(), "claude-code".into(), "codex".into()]).unwrap();
        assert_eq!(order(&db), ["codex", "claude-code"]);
        assert_eq!(limited_until(&db, "codex", 100), None);
        mark_limited(&db, "codex", 100);
        assert_eq!(limited_until(&db, "codex", 100), Some(100 + DEFAULT_COOLDOWN_SECS));
        assert_eq!(limited_until(&db, "codex", 100 + DEFAULT_COOLDOWN_SECS + 1), None, "expires by itself");
        clear_limit(&db, "codex");
        assert_eq!(limited_until(&db, "codex", 100), None);
    }

    #[test]
    fn the_handover_note_is_short_neutral_and_points_at_the_project_files() {
        let n = handover_note("claude-code", "codex", Some("Before Claude: make the title bigger"), &["a".into(), "b".into(), "c".into(), "d".into()]);
        assert!(n.contains("Claude Code before; you are Codex CLI now") && n.contains("STUDIO.md") && n.contains(".studio/screens.md"));
        assert!(n.contains("\"b\"; \"c\"; \"d\"") && !n.contains("\"a\""), "only the last three requests: {n}");
        assert!(handover_note("claude-code", "gemini", None, &[]).len() < 260);
    }
}

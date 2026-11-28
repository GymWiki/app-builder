//! Automatische modelkeuze (fase 11a). Per opdracht wordt een moeilijkheidsklasse bepaald uit goedkope signalen (geen
//! tokens), waarna het goedkoopste model wordt gekozen dat die klasse redelijk aankan. Alles wat een keuze bepaalt staat
//! als data in `engines/<engine>.json` (modelnamen, klassen, gewichten, drempels), zodat het zonder app-update kan worden
//! bijgewerkt: een bestand `engines/<engine>.json` in de app-datamap gaat voor het ingebouwde bestand.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Tier {
    pub id: String,
    /// Aliassen of namen in volgorde van voorkeur; de eerste die de CLI kent wordt gebruikt.
    pub models: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct KindRule {
    pub id: String,
    pub weight: u32,
    pub words: Vec<String>,
    /// Bestaat deze projectskill, dan is het een bekend patroon en wordt de klasse lichter.
    #[serde(default)]
    pub skill: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Thresholds {
    pub normal: u32,
    pub complex: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Signals {
    pub scope: BTreeMap<String, u32>,
    pub kinds: Vec<KindRule>,
    pub files_per_point: u32,
    pub files_max_points: u32,
    /// Lange, uitgebreide opdrachten (veel eisen in één verzoek) zijn zelden klein: één punt per zoveel tekens.
    #[serde(default)]
    pub length_per_point: u32,
    #[serde(default)]
    pub length_max_points: u32,
    pub previous_failure: u32,
    pub known_pattern_discount: u32,
    pub thresholds: Thresholds,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Escalation {
    pub max_per_task: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Downgrade {
    pub window: usize,
    pub min_clean: usize,
    /// Strengere (of mildere) drempel voor één klasse. De evaluatie liet zien dat complexe taken op lichtere modellen vaak een
    /// reparatie nodig hebben (sonnet 33% en haiku 50% meteen goed, opus 100%), dus daar is meer bewijs nodig.
    #[serde(default)]
    pub per_class: BTreeMap<String, DowngradeRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DowngradeRule {
    pub window: usize,
    pub min_clean: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Planning {
    pub enabled: bool,
    pub plan_tier: String,
    pub exec_tier: String,
    pub min_files: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EngineConfig {
    pub id: String,
    pub name: String,
    pub binary: String,
    pub model_flag: String,
    /// Van licht naar sterk.
    pub tiers: Vec<Tier>,
    pub class_map: BTreeMap<String, String>,
    pub helper_tier: String,
    /// Ondergrens per doel (bv. "release"), nooit door de statistiek te verlagen.
    pub floors: BTreeMap<String, String>,
    pub signals: Signals,
    pub escalation: Escalation,
    pub downgrade: Downgrade,
    pub planning: Planning,
}

const CLAUDE: &str = include_str!("../../engines/claude.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Class {
    Trivial,
    Normal,
    Complex,
}

impl Class {
    pub fn as_str(self) -> &'static str {
        match self {
            Class::Trivial => "trivial",
            Class::Normal => "normal",
            Class::Complex => "complex",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    /// Een gewone opdracht van de gebruiker.
    Work,
    /// Kleine hulptaak (changelog, store-tekst, samenvatting): altijd het lichtste model.
    Helper,
    /// Hoort bij publiceren: nooit onder de ingestelde ondergrens.
    Release,
}

impl Purpose {
    pub fn parse(s: Option<&str>) -> Purpose {
        match s {
            Some("helper") => Purpose::Helper,
            Some("release") => Purpose::Release,
            _ => Purpose::Work,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Purpose::Work => "work",
            Purpose::Helper => "helper",
            Purpose::Release => "release",
        }
    }
}

impl EngineConfig {
    pub fn parse(src: &str) -> Result<EngineConfig, String> {
        let c: EngineConfig = serde_json::from_str(src).map_err(|e| e.to_string())?;
        c.validate()?;
        Ok(c)
    }

    pub fn embedded() -> EngineConfig {
        Self::parse(CLAUDE).expect("the built-in engine configuration is valid")
    }

    /// Het bestand in de app-datamap gaat voor; is het kapot, dan blijft het ingebouwde in gebruik (met een logregel).
    pub fn load(data_dir: &Path, id: &str) -> EngineConfig {
        let path = data_dir.join("engines").join(format!("{id}.json"));
        if let Ok(text) = std::fs::read_to_string(&path) {
            match Self::parse(&text) {
                Ok(c) if c.id == id => return c,
                Ok(_) => log::warn!("engine config {} has another id; using the built-in one", path.display()),
                Err(e) => log::warn!("engine config {} is invalid ({e}); using the built-in one", path.display()),
            }
        }
        Self::embedded()
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.tiers.is_empty() || self.tiers.iter().any(|t| t.models.is_empty() || t.models.iter().any(|m| m.trim().is_empty() || m.starts_with('-'))) {
            return Err("every tier needs at least one model name".into());
        }
        let known = |t: &str| self.tier_index(t).is_some();
        for c in ["trivial", "normal", "complex"] {
            if !self.class_map.get(c).is_some_and(|t| known(t)) {
                return Err(format!("class {c} must map to a known tier"));
            }
        }
        if !known(&self.helper_tier) {
            return Err("helperTier is not a known tier".into());
        }
        if self.floors.values().any(|t| !known(t)) {
            return Err("a floor refers to an unknown tier".into());
        }
        if self.signals.thresholds.normal > self.signals.thresholds.complex {
            return Err("thresholds: normal must not be above complex".into());
        }
        if self.planning.enabled && (!known(&self.planning.plan_tier) || !known(&self.planning.exec_tier)) {
            return Err("planning refers to an unknown tier".into());
        }
        for lvl in ["small", "screen", "shared", "project"] {
            if !self.signals.scope.contains_key(lvl) {
                return Err(format!("signals.scope needs {lvl}"));
            }
        }
        Ok(())
    }

    pub fn tier_index(&self, id: &str) -> Option<usize> {
        self.tiers.iter().position(|t| t.id == id)
    }

    pub fn tier_of_class(&self, c: Class) -> usize {
        self.class_map.get(c.as_str()).and_then(|t| self.tier_index(t)).unwrap_or(0)
    }

    pub fn strongest(&self) -> usize {
        self.tiers.len() - 1
    }

    /// Eén niveau zwaarder, of `None` als dit al het sterkste is.
    pub fn escalate(&self, tier: usize) -> Option<usize> {
        (tier < self.strongest()).then_some(tier + 1)
    }

    /// Welk alias bij een tier hoort: de eerste kandidaat die de CLI kent (`available`).
    pub fn resolve(&self, tier: usize, available: &dyn Fn(&str) -> bool) -> Option<String> {
        let try_tier = |i: usize| self.tiers.get(i).and_then(|t| t.models.iter().find(|m| available(m)).cloned());
        // Kent de CLI niets van deze tier, dan de dichtstbijzijnde: eerst zwaarder (kwaliteit), dan lichter.
        try_tier(tier).or_else(|| (tier + 1..self.tiers.len()).find_map(try_tier)).or_else(|| (0..tier).rev().find_map(try_tier))
    }
}

// ───────────── moeilijkheidsklasse ─────────────

pub struct AssessInput<'a> {
    /// small | screen | shared | project
    pub level: &'a str,
    pub text: &'a str,
    /// Aantal bestanden in het bereik van de opdracht.
    pub files: usize,
    pub previous_failed: bool,
    pub has_skill: &'a dyn Fn(&str) -> bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Assessment {
    pub class: Class,
    pub score: u32,
    pub kind: Option<String>,
    pub reasons: Vec<String>,
}

pub fn assess(cfg: &EngineConfig, i: &AssessInput) -> Assessment {
    let s = &cfg.signals;
    let lower = i.text.to_lowercase();
    let mut reasons = vec![];
    let scope = s.scope.get(i.level).copied().unwrap_or(0);
    let mut score = scope;
    reasons.push(format!("scope {} = {scope}", i.level));

    // Soort taak: het zwaarste dat past telt.
    // Bij gelijk gewicht gaat de eerste in de configuratie voor (specifieker boven algemener).
    let best = s.kinds.iter().filter(|k| k.words.iter().any(|w| lower.contains(&w.to_lowercase()))).fold(None::<&KindRule>, |acc, k| match acc {
        Some(a) if a.weight >= k.weight => Some(a),
        _ => Some(k),
    });
    let mut kind = None;
    if let Some(k) = best {
        score += k.weight;
        kind = Some(k.id.clone());
        reasons.push(format!("kind {} = {}", k.id, k.weight));
    }
    let file_points = (i.files as u32).checked_div(s.files_per_point).unwrap_or(0).min(s.files_max_points);
    if file_points > 0 {
        score += file_points;
        reasons.push(format!("{} files = {file_points}", i.files));
    }
    let length_points = (i.text.chars().count() as u32).checked_div(s.length_per_point).unwrap_or(0).min(s.length_max_points);
    if length_points > 0 {
        score += length_points;
        reasons.push(format!("long request = {length_points}"));
    }
    if i.previous_failed {
        score += s.previous_failure;
        reasons.push(format!("previous attempt failed = {}", s.previous_failure));
    }
    // Een bekend patroon (er is een projectskill voor) maakt het lichter, maar nooit lichter dan wat de scope vraagt.
    if let Some(skill) = best.and_then(|k| k.skill.as_deref()) {
        if score > 0 && (i.has_skill)(skill) {
            let d = s.known_pattern_discount.min(score);
            score -= d;
            reasons.push(format!("known pattern ({skill}) = -{d}"));
        }
    }
    let class = if score >= s.thresholds.complex {
        Class::Complex
    } else if score >= s.thresholds.normal {
        Class::Normal
    } else {
        Class::Trivial
    };
    Assessment { class, score, kind, reasons }
}

// ───────────── keuze van het niveau ─────────────

/// Eerder resultaat van een run in dezelfde klasse (nieuwste eerst).
#[derive(Debug, Clone, PartialEq)]
pub struct HistoryRow {
    pub tier: String,
    /// Meteen goed: geslaagd zonder reparatie, opschaling of terugdraaien.
    pub clean: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Choice {
    pub tier: usize,
    pub tier_id: String,
    pub reason: String,
    /// Alleen bij een handmatige override met een eigen modelnaam (ontwikkelaarsmodus, evaluatie): dat model, los van de tiers.
    pub explicit_model: Option<String>,
}

/// Mag de klasse een niveau omlaag? Alleen na `min_clean` keer meteen goed binnen het venster, en zonder een mislukking op het
/// lichtere niveau in de twee vensters daarvoor (anders pendelt de keuze).
pub fn downgrade_ok(cfg: &EngineConfig, class: Class, mapped: usize, history: &[HistoryRow]) -> bool {
    let rule = cfg.downgrade.per_class.get(class.as_str()).cloned().unwrap_or(DowngradeRule { window: cfg.downgrade.window, min_clean: cfg.downgrade.min_clean });
    if mapped == 0 || rule.window == 0 || history.len() < rule.window {
        return false;
    }
    let clean = history.iter().take(rule.window).filter(|h| h.clean).count();
    if clean < rule.min_clean {
        return false;
    }
    let lower = &cfg.tiers[mapped - 1].id;
    !history.iter().take(rule.window * 2).any(|h| &h.tier == lower && !h.clean)
}

pub fn choose(cfg: &EngineConfig, a: &Assessment, purpose: Purpose, history: &[HistoryRow], override_value: Option<&str>) -> Choice {
    let named = |i: usize, reason: String| Choice { tier: i, tier_id: cfg.tiers[i].id.clone(), reason, explicit_model: None };
    // Alleen de ontwikkelaarsmodus geeft een override door; "auto" of leeg betekent: gewoon kiezen.
    if let Some(o) = override_value.filter(|o| !o.is_empty() && *o != "auto") {
        if let Some(i) = cfg.tier_index(o) {
            return named(i, format!("override {o}"));
        }
        if let Some(i) = cfg.tiers.iter().position(|t| t.models.iter().any(|m| m == o)) {
            return named(i, format!("override {o}"));
        }
        // Een eigen modelnaam (bv. een model dat nog in geen enkele tier staat): alleen als het veilig als argument is.
        if !o.starts_with('-') && o.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | ':' | '-' | '[' | ']')) {
            let mut c = named(cfg.strongest(), format!("override {o}"));
            c.tier_id = "custom".into();
            c.explicit_model = Some(o.to_string());
            return c;
        }
    }
    let floor = |i: usize| match purpose {
        Purpose::Release => cfg.floors.get("release").and_then(|f| cfg.tier_index(f)).map(|f| i.max(f)).unwrap_or(i),
        _ => i,
    };
    if purpose == Purpose::Helper {
        let i = cfg.tier_index(&cfg.helper_tier).unwrap_or(0);
        return named(i, "helper task".into());
    }
    let mapped = cfg.tier_of_class(a.class);
    let (tier, why) = if purpose == Purpose::Work && downgrade_ok(cfg, a.class, mapped, history) {
        (mapped - 1, format!("{} class, lowered by statistics", a.class.as_str()))
    } else {
        (mapped, format!("{} class", a.class.as_str()))
    };
    let floored = floor(tier);
    let why = if floored != tier { format!("{why}, raised to the release minimum") } else { why };
    named(floored, why)
}

/// Plannen met een sterk model en uitvoeren met een lichter model: alleen voor grote taken, en alleen als het in de
/// configuratie aanstaat (het moet eerst netto goedkoper blijken).
pub fn should_plan(cfg: &EngineConfig, a: &Assessment, purpose: Purpose, files: usize) -> bool {
    cfg.planning.enabled && purpose == Purpose::Work && a.class == Class::Complex && files >= cfg.planning.min_files
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> EngineConfig {
        EngineConfig::embedded()
    }
    fn a(level: &str, text: &str, files: usize, failed: bool, skills: &[&str]) -> Assessment {
        let have = |s: &str| skills.contains(&s);
        assess(&cfg(), &AssessInput { level, text, files, previous_failed: failed, has_skill: &have })
    }

    #[test]
    fn the_built_in_config_is_valid_and_maps_classes_to_tiers() {
        let c = cfg();
        assert_eq!(c.tiers.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), ["light", "mid", "strong"]);
        assert_eq!((c.tier_of_class(Class::Trivial), c.tier_of_class(Class::Normal), c.tier_of_class(Class::Complex)), (0, 1, 2));
        assert!(!c.planning.enabled, "planning stays off until it proves cheaper");
    }

    #[test]
    fn small_text_and_style_tweaks_are_trivial() {
        for t in ["Change the title to Game Night", "Make the player name bold", "Change the tooltip of the reset button", "maak de kleur groen"] {
            let r = a("small", t, 1, false, &[]);
            assert_eq!(r.class, Class::Trivial, "{t}: {r:?}");
        }
    }

    #[test]
    fn a_bigger_change_in_one_screen_is_normal() {
        let r = a("screen", "Show the total of all scores under the title", 3, false, &[]);
        assert_eq!(r.class, Class::Normal, "{r:?}");
        assert_eq!(a("small", "Bug: the minus button raises the score. Fix it.", 2, false, &[]).class, Class::Normal);
    }

    #[test]
    fn shared_and_project_work_and_hard_kinds_are_complex() {
        assert_eq!(a("shared", "Add dark mode with a switch", 6, false, &[]).class, Class::Complex);
        assert_eq!(a("project", "Rename the score everywhere", 9, false, &[]).class, Class::Complex);
        assert_eq!(a("screen", "Add login with email", 4, false, &[]).class, Class::Complex, "auth/db is hard even in one screen");
        assert_eq!(a("screen", "Refactor this screen", 4, false, &[]).class, Class::Complex);
    }

    #[test]
    fn many_files_a_failed_previous_attempt_and_known_patterns_move_the_class() {
        let base = a("small", "Change the title", 1, false, &[]);
        assert_eq!(base.class, Class::Trivial);
        let many = a("small", "Change the title", 40, false, &[]);
        assert_eq!((many.score, many.class), (1, Class::Normal), "a task that touches many files is not trivial");
        let failed = a("small", "Change the title", 1, true, &[]);
        assert_eq!(failed.class, Class::Normal, "{failed:?}");
        // Een bekend patroon (skill aanwezig) maakt een nieuw scherm lichter dan zonder skill.
        let without = a("screen", "Add a new screen for settings", 3, false, &[]);
        let with = a("screen", "Add a new screen for settings", 3, false, &["studio-screen"]);
        assert!(with.score < without.score && with.reasons.iter().any(|r| r.contains("known pattern")), "{without:?} {with:?}");
        // ...maar nooit onder nul.
        assert_eq!(a("small", "Change the title", 1, false, &["studio-store-listing"]).score, 0);
    }

    #[test]
    fn a_long_multi_part_request_is_not_treated_as_normal() {
        // De evaluatie liet zien dat zo'n verzoek (toernooi met rondes, standen en tests) als "normaal" werd gezien.
        let long = "Add a tournament mode: the players are paired in rounds (everyone plays everyone once), a screen shows the matches of the current round, and after a score is entered for every match the standings (wins and points) update. Put the pairing and standings logic in plain Dart code and cover it with unit tests.";
        let r = a("screen", long, 3, false, &[]);
        assert_eq!(r.class, Class::Complex, "{r:?}");
        assert!(r.reasons.iter().any(|x| x.contains("long request")));
        assert_eq!(a("small", "Change the title to Game Night", 1, false, &[]).class, Class::Trivial, "short requests are unaffected");
    }

    #[test]
    fn the_heaviest_matching_kind_wins() {
        let r = a("small", "Rename the label and refactor everywhere", 1, false, &[]);
        assert_eq!(r.kind.as_deref(), Some("refactor"));
    }

    #[test]
    fn choose_follows_the_class_map_and_the_override_only_when_given() {
        let c = cfg();
        let triv = a("small", "Change the title", 1, false, &[]);
        let comp = a("shared", "Add dark mode", 6, false, &[]);
        assert_eq!(choose(&c, &triv, Purpose::Work, &[], None).tier_id, "light");
        assert_eq!(choose(&c, &comp, Purpose::Work, &[], None).tier_id, "strong");
        assert_eq!(choose(&c, &comp, Purpose::Work, &[], Some("auto")).tier_id, "strong");
        assert_eq!(choose(&c, &comp, Purpose::Work, &[], Some("light")).tier_id, "light");
        assert_eq!(choose(&c, &comp, Purpose::Work, &[], Some("sonnet")).tier_id, "mid", "an explicit model name finds its tier");
        let custom = choose(&c, &comp, Purpose::Work, &[], Some("some-new-model"));
        assert_eq!((custom.tier_id.as_str(), custom.explicit_model.as_deref()), ("custom", Some("some-new-model")), "an own model name is passed on as it is");
        assert_eq!(choose(&c, &comp, Purpose::Work, &[], Some("--oops")).tier_id, "strong", "something that looks like a flag is ignored");
        assert_eq!(choose(&c, &comp, Purpose::Work, &[], Some("a b; rm -rf")).tier_id, "strong", "and so is anything with odd characters");
    }

    #[test]
    fn helper_tasks_use_the_lightest_model_and_release_work_has_a_floor() {
        let c = cfg();
        let comp = a("shared", "Write the changelog", 6, false, &[]);
        assert_eq!(choose(&c, &comp, Purpose::Helper, &[], None).tier_id, "light");
        let triv = a("small", "Change the title", 1, false, &[]);
        let r = choose(&c, &triv, Purpose::Release, &[], None);
        assert_eq!(r.tier_id, "mid");
        assert!(r.reason.contains("release minimum"));
        // De statistiek kan de ondergrens nooit verlagen.
        let clean: Vec<HistoryRow> = (0..10).map(|_| HistoryRow { tier: "mid".into(), clean: true }).collect();
        let norm = a("screen", "Show the total", 3, false, &[]);
        assert_eq!(choose(&c, &norm, Purpose::Release, &clean, None).tier_id, "mid");
    }

    #[test]
    fn escalation_goes_one_tier_up_and_stops_at_the_strongest() {
        let c = cfg();
        assert_eq!(c.escalate(0), Some(1));
        assert_eq!(c.escalate(1), Some(2));
        assert_eq!(c.escalate(2), None);
    }

    fn rows(tier: &str, clean: &[bool]) -> Vec<HistoryRow> {
        clean.iter().map(|c| HistoryRow { tier: tier.into(), clean: *c }).collect()
    }

    #[test]
    fn repeated_clean_success_lowers_the_class_one_tier_and_a_failure_brings_it_back() {
        let c = cfg();
        let norm = a("screen", "Show the total", 3, false, &[]);
        assert_eq!(choose(&c, &norm, Purpose::Work, &rows("mid", &[true; 4]), None).tier_id, "mid", "not enough history yet");
        assert_eq!(choose(&c, &norm, Purpose::Work, &rows("mid", &[true; 5]), None).tier_id, "light", "five clean runs: try lighter");
        assert_eq!(choose(&c, &norm, Purpose::Work, &rows("mid", &[true, true, true, true, false]), None).tier_id, "mid", "one messy run in the window: stay");
        // Op het lichtere niveau mislukt: terug omhoog, en niet meteen opnieuw proberen.
        let mut h = rows("light", &[true, true, true, true, true]);
        h.push(HistoryRow { tier: "light".into(), clean: false });
        assert_eq!(choose(&c, &norm, Purpose::Work, &h, None).tier_id, "mid", "a failure at the lighter tier blocks lowering for two windows");
        // Trivial kan niet lager dan het lichtste niveau.
        let triv = a("small", "Change the title", 1, false, &[]);
        assert_eq!(choose(&c, &triv, Purpose::Work, &rows("light", &[true; 9]), None).tier_id, "light");
    }

    #[test]
    fn complex_work_needs_more_clean_runs_before_it_is_lowered() {
        let c = cfg();
        let comp = a("shared", "Add dark mode with a switch", 6, false, &[]);
        assert_eq!(choose(&c, &comp, Purpose::Work, &rows("strong", &[true; 5]), None).tier_id, "strong", "five clean runs are enough for normal work, not for complex work");
        assert_eq!(choose(&c, &comp, Purpose::Work, &rows("strong", &[true; 7]), None).tier_id, "strong");
        assert_eq!(choose(&c, &comp, Purpose::Work, &rows("strong", &[true; 8]), None).tier_id, "mid", "eight in a row: one tier lighter");
        let mut h = rows("strong", &[true; 7]);
        h.insert(3, HistoryRow { tier: "strong".into(), clean: false });
        assert_eq!(choose(&c, &comp, Purpose::Work, &h, None).tier_id, "strong", "one messy run in the window of eight blocks it");
    }

    /// De opdrachten van de evaluatieset moeten door de signalen in de verwachte klasse vallen (afgesteld op echte cijfers).
    #[test]
    fn the_evaluation_prompts_land_in_the_expected_class() {
        use crate::evals::load_tasks_of;
        let c = cfg();
        let none = |_: &str| false;
        let class_at = |level: &str, text: &str| assess(&c, &AssessInput { level, text, files: 3, previous_failed: false, has_skill: &none }).class;
        for t in load_tasks_of("hard").tasks {
            match t.expect.as_str() {
                // Een moeilijke taak mag nooit lichter dan "complex" worden gezien als het scherm al bekend is.
                "complex" => assert_eq!(class_at("screen", &t.prompt), Class::Complex, "{}", t.id),
                // Een gewone taak mag niet zwaarder dan "normaal" zijn als de scope klein is.
                _ => assert!(class_at("small", &t.prompt) <= Class::Normal, "{}", t.id),
            }
        }
        for t in load_tasks_of("basic").tasks {
            if t.expect == "trivial" {
                assert_eq!(class_at("small", &t.prompt), Class::Trivial, "{}", t.id);
            }
        }
    }

    #[test]
    fn resolve_picks_the_first_model_the_cli_knows_and_falls_back_to_the_nearest_tier() {
        let mut c = cfg();
        c.tiers[0].models = vec!["tiny".into(), "haiku".into()];
        assert_eq!(c.resolve(0, &|m| m == "haiku").as_deref(), Some("haiku"));
        assert_eq!(c.resolve(0, &|m| m == "tiny" || m == "haiku").as_deref(), Some("tiny"), "preference order");
        assert_eq!(c.resolve(0, &|m| m == "sonnet").as_deref(), Some("sonnet"), "no light model: the next heavier one");
        assert_eq!(c.resolve(2, &|m| m == "haiku").as_deref(), Some("haiku"), "only a lighter one exists");
        assert_eq!(c.resolve(1, &|_| false), None, "nothing known: leave the choice to the CLI");
    }

    #[test]
    fn planning_only_for_big_complex_tasks_and_only_when_enabled() {
        let mut c = cfg();
        let comp = a("project", "Refactor everything", 20, false, &[]);
        assert!(!should_plan(&c, &comp, Purpose::Work, 20));
        c.planning.enabled = true;
        assert!(should_plan(&c, &comp, Purpose::Work, 20));
        assert!(!should_plan(&c, &comp, Purpose::Work, 3), "too few files");
        assert!(!should_plan(&c, &comp, Purpose::Helper, 20));
        assert!(!should_plan(&c, &a("small", "Change the title", 1, false, &[]), Purpose::Work, 20));
    }

    #[test]
    fn broken_configs_are_rejected_and_an_override_file_replaces_the_built_in_one() {
        let mut v: serde_json::Value = serde_json::from_str(CLAUDE).unwrap();
        v["classMap"]["complex"] = "nope".into();
        assert!(EngineConfig::parse(&v.to_string()).unwrap_err().contains("complex"));
        let mut v: serde_json::Value = serde_json::from_str(CLAUDE).unwrap();
        v["tiers"][0]["models"] = serde_json::json!([]);
        assert!(EngineConfig::parse(&v.to_string()).is_err());
        let mut v: serde_json::Value = serde_json::from_str(CLAUDE).unwrap();
        v["tiers"][0]["models"] = serde_json::json!(["--dangerous"]);
        assert!(EngineConfig::parse(&v.to_string()).is_err(), "a model name can never look like a flag");
        assert!(EngineConfig::parse("not json").is_err());

        let dir = std::env::temp_dir().join(format!("studio-engines-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("engines")).unwrap();
        assert_eq!(EngineConfig::load(&dir, "claude"), cfg(), "no override file: built-in");
        let mut v: serde_json::Value = serde_json::from_str(CLAUDE).unwrap();
        v["tiers"][0]["models"] = serde_json::json!(["my-light-model"]);
        std::fs::write(dir.join("engines/claude.json"), v.to_string()).unwrap();
        assert_eq!(EngineConfig::load(&dir, "claude").tiers[0].models, ["my-light-model"], "updated without an app update");
        std::fs::write(dir.join("engines/claude.json"), "{ broken").unwrap();
        assert_eq!(EngineConfig::load(&dir, "claude"), cfg(), "a broken override falls back to the built-in file");
    }
}

//! Automatische keuze van het werkgebied van een run (fase 7). De gebruiker kiest nooit een niveau: Studio leidt het af
//! uit de selectie, het huidige scherm, trefwoorden of (alleen als dat niets oplevert) één goedkope AI-vraag.
//!
//! Niveaus: Klein (één bestand), Scherm (de map van het scherm), Gedeeld (ook thema, modellen, pubspec) en Project
//! (alles). Tijdens een run bewaakt Studio de grens via de permissie-tool; Claude vraagt met `breid_bereik_uit` om meer.

use crate::index::CodeIndex;
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    Small,
    Screen,
    Shared,
    Project,
}

impl Level {
    pub fn as_str(self) -> &'static str {
        match self {
            Level::Small => "small",
            Level::Screen => "screen",
            Level::Shared => "shared",
            Level::Project => "project",
        }
    }
    pub fn parse(s: &str) -> Option<Level> {
        match s.trim().to_lowercase().as_str() {
            "small" | "klein" => Some(Level::Small),
            "screen" | "scherm" => Some(Level::Screen),
            "shared" | "gedeeld" => Some(Level::Shared),
            "project" => Some(Level::Project),
            _ => None,
        }
    }
    pub fn next(self) -> Level {
        match self {
            Level::Small => Level::Screen,
            Level::Screen => Level::Shared,
            Level::Shared | Level::Project => Level::Project,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Why {
    Selection,
    Current,
    Keyword,
    Ai,
    Fallback,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Decision {
    pub level: Level,
    pub screen: Option<String>,
    pub why: Why,
}

// ───────────── niveau-keuze ─────────────

const PROJECT_WORDS: &[&str] = &[
    "whole app", "entire app", "all screens", "every screen", "restructure", "rewrite", "from scratch", "new app", "database", "login", "log in", "sign in", "sign up",
    "authentication", "backend", "navigation", "state management", "hele app", "alle schermen", "elk scherm", "herstructureer", "opnieuw opbouwen", "inloggen", "registreren",
    "authenticatie", "navigatie", "nieuwe app", "accounts",
];
const SHARED_WORDS: &[&str] = &[
    "theme", "thema", "everywhere", "overal", "shared", "gedeeld", "package", "dependency", "pubspec", "font", "lettertype", "logo", "app icon", "app-icoon", "model",
    "in every", "in alle", "all buttons", "alle knoppen", "app bar", "appbar", "dark mode", "donker thema", "language", "taal", "translation", "vertaling", "new screen", "nieuw scherm",
    "extra scherm", "another screen", "second screen", "third screen", "tweede scherm", "derde scherm", "new page", "nieuwe pagina", "add a page", "add a screen", "voeg een scherm",
];
const SMALL_WORDS: &[&str] = &[
    "color", "colour", "kleur", "text", "tekst", "label", "bigger", "smaller", "larger", "groter", "kleiner", "spacing", "margin", "padding", "rename", "hernoem", "title", "titel",
    "bold", "vet", "align", "uitlijnen", "typo", "icon", "icoon", "rounded", "afgerond", "wording",
];

/// Bestandspaden in de tekst ("lib/core/models/player.dart") zeggen niets over het niveau.
fn strip_paths(text: &str) -> String {
    text.split_whitespace().filter(|w| !w.contains('/') && !w.trim_matches(['`', '"', '\'', '.', ',']).ends_with(".dart")).collect::<Vec<_>>().join(" ")
}

fn has_any(text: &str, words: &[&str]) -> bool {
    words.iter().any(|w| text.contains(w))
}

/// Niveau uit de tekst, gegeven of er een scherm bekend is.
pub fn level_from_text(text: &str, has_screen: bool) -> Option<Level> {
    let t = strip_paths(text).to_lowercase();
    if has_any(&t, PROJECT_WORDS) {
        return Some(Level::Project);
    }
    if has_any(&t, SHARED_WORDS) {
        return Some(Level::Shared);
    }
    if has_screen {
        return Some(if has_any(&t, SMALL_WORDS) && t.chars().count() <= 200 { Level::Small } else { Level::Screen });
    }
    None
}

fn words(text: &str) -> Vec<String> {
    text.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(String::from).collect()
}

fn snake(class: &str) -> String {
    let mut out = String::new();
    for (i, c) in class.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            out.push('_');
        }
        out.extend(c.to_lowercase());
    }
    out
}

/// De naam zoals een gebruiker hem zegt: `SettingsScreen` -> `settings`.
fn short_name(class: &str) -> String {
    let base = class.strip_suffix("Screen").or_else(|| class.strip_suffix("Page")).unwrap_or(class);
    snake(base)
}

/// Schermen die in de tekst genoemd worden (op klassenaam, korte naam, mapnaam of route).
pub fn mentioned_screens<'a>(idx: &'a CodeIndex, text: &str) -> Vec<&'a str> {
    let lower = text.to_lowercase();
    let ws = words(text);
    idx.screens
        .iter()
        .filter(|s| {
            let short = short_name(&s.class);
            let spaced = short.replace('_', " ");
            lower.contains(&s.class.to_lowercase())
                || (short.len() >= 3 && (ws.contains(&short) || ws.contains(&short.replace('_', "")) || lower.contains(&spaced)))
                || s.route.as_deref().is_some_and(|r| r.len() > 1 && lower.contains(r))
        })
        .map(|s| s.class.as_str())
        .collect()
}

fn selected_screen<'a>(idx: &'a CodeIndex, context: Option<&str>) -> Option<&'a str> {
    let c = context?;
    idx.screens.iter().find(|s| c.contains(&format!("\"{}\"", s.class)) || c.contains(&s.file)).map(|s| s.class.as_str())
}

const DEICTIC: &[&str] = &["this screen", "this page", "dit scherm", "deze pagina", "this one", "here", "hier", "this"];

/// Leest het antwoord van de AI-vraag: `{"level":"screen","screen":"HomeScreen"}` (met eventuele omringende tekst).
pub fn parse_ai(answer: &str, idx: &CodeIndex) -> Option<(Level, Option<String>)> {
    let start = answer.find('{')?;
    let end = answer.rfind('}')?;
    let v: Value = serde_json::from_str(answer.get(start..=end)?).ok()?;
    let level = Level::parse(v["level"].as_str()?)?;
    let screen = v["screen"].as_str().and_then(|n| idx.screen(n)).map(|s| s.class.clone());
    Some((level, screen))
}

pub fn ai_question(idx: &CodeIndex, text: &str) -> String {
    let screens: Vec<String> = idx.screens.iter().map(|s| s.class.clone()).collect();
    format!(
        "Classify this request for a Flutter app. Screens: {}.\nRequest: {}\n\
         Answer with ONLY a JSON object like {{\"level\":\"small|screen|shared|project\",\"screen\":\"<ScreenClass or null>\"}}.\n\
         small = one small tweak in one screen; screen = bigger change in one screen; shared = theme, shared widgets, models, packages or a new screen; project = whole app, navigation, data, accounts.",
        screens.join(", "),
        text.chars().take(600).collect::<String>()
    )
}

/// Volgorde: geselecteerd element, huidig scherm, trefwoord, één AI-vraag, terugval. `ask` krijgt de vraag en geeft het
/// antwoord (of `None` als er geen engine beschikbaar is).
pub fn decide(idx: &CodeIndex, text: &str, context: Option<&str>, current: Option<&str>, ask: &mut dyn FnMut(&str) -> Option<String>) -> Decision {
    let t = text.to_lowercase();
    let pick = |class: &str, why: Why| {
        let level = level_from_text(text, true).unwrap_or(Level::Screen);
        Decision { level, screen: (level != Level::Project).then(|| class.to_string()).or(Some(class.to_string())), why }
    };
    if let Some(c) = selected_screen(idx, context) {
        return pick(c, Why::Selection);
    }
    if let Some(cur) = current.filter(|c| idx.screen(c).is_some()) {
        if DEICTIC.iter().any(|d| words(&t).contains(&d.to_string()) || t.contains(d) && d.contains(' ')) {
            return pick(cur, Why::Current);
        }
    }
    let named = mentioned_screens(idx, text);
    if named.len() == 1 {
        return pick(named[0], Why::Keyword);
    }
    if named.len() > 1 {
        // Meerdere schermen genoemd: minstens het gedeelde niveau.
        let level = level_from_text(text, false).unwrap_or(Level::Shared).max(Level::Shared);
        return Decision { level, screen: None, why: Why::Keyword };
    }
    if let Some(level) = level_from_text(text, false) {
        return Decision { level, screen: None, why: Why::Keyword };
    }
    if idx.screens.len() == 1 {
        return pick(&idx.screens[0].class, Why::Fallback);
    }
    if let Some((level, screen)) = ask(&ai_question(idx, text)).and_then(|a| parse_ai(&a, idx)) {
        return Decision { level, screen, why: Why::Ai };
    }
    match current.filter(|c| idx.screen(c).is_some()) {
        Some(c) => pick(c, Why::Fallback),
        None => Decision { level: Level::Project, screen: None, why: Why::Fallback },
    }
}

// ───────────── toegestane bestanden ─────────────

#[derive(Debug, Clone)]
pub struct RunScope {
    pub level: Level,
    pub screen: Option<String>,
    files: BTreeSet<String>,
    prefixes: Vec<String>,
    /// (van, naar, reden)
    pub expansions: Vec<(Level, Level, String)>,
    pub denied: Vec<String>,
    /// Alleen de flow van de Aankopen-pagina mag in `lib/core/purchases/` en `lib/features/paywall/` schrijven.
    pub purchases_ok: bool,
}

const ALWAYS_FILES: &[&str] = &["STUDIO.md"];
const ALWAYS_PREFIXES: &[&str] = &[".studio/"];

impl RunScope {
    pub fn new(idx: &CodeIndex, d: &Decision) -> Self {
        let mut s = RunScope { level: d.level, screen: d.screen.clone(), files: BTreeSet::new(), prefixes: vec![], expansions: vec![], denied: vec![], purchases_ok: false };
        s.rebuild(idx);
        s
    }

    /// Werkgebied voor een bouwtaak (fase 14): alleen deze mappen en bestanden (plus `test/` en de vaste projectbestanden).
    pub fn for_paths(prefixes: &[String], files: &[String]) -> Self {
        let mut s = RunScope { level: Level::Screen, screen: None, files: BTreeSet::new(), prefixes: vec![], expansions: vec![], denied: vec![], purchases_ok: false };
        s.prefixes.extend(prefixes.iter().cloned());
        s.prefixes.push("test/".into());
        s.files.extend(files.iter().cloned());
        s
    }

    fn rebuild(&mut self, idx: &CodeIndex) {
        let extra_files: BTreeSet<String> = self.files.iter().filter(|f| f.starts_with('+')).cloned().collect();
        self.files.clear();
        self.prefixes.clear();
        let entry = self.screen.as_deref().and_then(|n| idx.screen(n));
        match self.level {
            Level::Small => {
                if let Some(e) = entry {
                    self.files.insert(e.file.clone());
                }
            }
            Level::Screen | Level::Shared => {
                if let Some(e) = entry {
                    match &e.feature_dir {
                        Some(fd) => self.prefixes.push(format!("{fd}/")),
                        None => {
                            self.files.insert(e.file.clone());
                        }
                    }
                }
                self.prefixes.push("test/".into());
                if self.level == Level::Shared {
                    self.prefixes.extend(["lib/core/".into(), "assets/".into()]);
                    self.files.insert("pubspec.yaml".into());
                    self.files.insert("lib/main.dart".into());
                    self.files.insert("lib/app.dart".into());
                }
            }
            Level::Project => {}
        }
        self.files.extend(extra_files);
    }

    pub fn permits(&self, rel: &str) -> bool {
        let rel = rel.trim_start_matches("./");
        if !self.purchases_ok && crate::purchases_flow::is_protected(rel) {
            return false;
        }
        self.level == Level::Project
            || ALWAYS_FILES.contains(&rel)
            || ALWAYS_PREFIXES.iter().any(|p| rel.starts_with(p))
            || self.files.contains(rel)
            || self.files.contains(&format!("+{rel}"))
            || self.prefixes.iter().any(|p| rel.starts_with(p.as_str()))
    }

    /// Absolute of relatieve padverwijzing uit een tool-aanroep, binnen het project.
    pub fn permits_path(&self, project_dir: &Path, p: &str) -> bool {
        let path = Path::new(p);
        let rel = if path.is_absolute() { path.strip_prefix(project_dir).map(|r| r.to_string_lossy().replace('\\', "/")) } else { Ok(p.replace('\\', "/")) };
        match rel {
            Ok(r) if !r.split('/').any(|c| c == "..") => self.permits(&r),
            _ => false,
        }
    }

    /// Verbreedt het gebied: naar een expliciet niveau, naar specifieke bestanden, of (zonder beide) één niveau omhoog.
    pub fn expand(&mut self, idx: &CodeIndex, to: Option<Level>, files: &[String], reason: &str) {
        let from = self.level;
        if files.is_empty() || to.is_some() {
            self.level = to.unwrap_or_else(|| from.next()).max(from);
        }
        for f in files {
            let f = f.trim_start_matches("./").replace('\\', "/");
            if !f.starts_with('/') && !f.contains("..") {
                self.files.insert(format!("+{f}"));
            }
        }
        self.rebuild(idx);
        self.expansions.push((from, self.level, reason.chars().take(200).collect()));
    }

    /// Bestanden uit een foutmelding mogen voor gerichte reparatie wel aangepast worden.
    pub fn allow_files(&mut self, files: &[String]) {
        for f in files {
            self.files.insert(format!("+{f}"));
        }
    }

    /// Gewijzigde bestanden die buiten het werkgebied vallen. Alleen code, tests, assets en pubspec tellen mee; afgeleide
    /// bestanden (pubspec.lock, .dart_tool, platformmappen) horen bij de tools zelf.
    pub fn breaches(&self, changed: &[String]) -> Vec<String> {
        changed
            .iter()
            .filter(|f| f.starts_with("lib/") || f.starts_with("test/") || f.starts_with("assets/") || f.as_str() == "pubspec.yaml")
            .filter(|f| !self.permits(f))
            .cloned()
            .collect()
    }

    pub fn describe(&self) -> String {
        let lvl = match self.level {
            Level::Small => "Small (one file)",
            Level::Screen => "Screen",
            Level::Shared => "Shared",
            Level::Project => "Project",
        };
        let target = self.screen.as_deref().map(|s| format!(" for {s}")).unwrap_or_default();
        if self.level == Level::Project {
            return format!("Scope: {lvl}{target}. You may edit anything in the project.");
        }
        let mut allowed: Vec<String> = self.files.iter().map(|f| f.trim_start_matches('+').to_string()).collect();
        allowed.extend(self.prefixes.iter().map(|p| format!("{p}**")));
        format!(
            "Scope: {lvl}{target}. You may edit only: {}. Everything else is read-only for this task. \
             If the task really needs a change outside that, FIRST call the tool breid_bereik_uit (reason, and optionally files or level), then continue. \
             Use scherm_index, afhankelijkheden and wie_gebruikt instead of searching the whole project.",
            allowed.join(", ")
        )
    }
}

/// Bestand, eerste en laatste regel (1-gebaseerd, inclusief).
pub type Range = (String, usize, usize);

/// Het lopende werkgebied per project; gedeeld tussen de chat-run en de MCP-server.
#[derive(Default)]
pub struct ScopeRegistry {
    runs: Mutex<HashMap<String, RunScope>>,
    /// Door de gebruiker gekozen regelbereiken (bestand, begin, eind) voor de volgende run; de controle achteraf gebruikt ze.
    selections: Mutex<HashMap<String, Vec<Range>>>,
    /// Projecten waarvan de volgende run de Aankopen-flow is (mag de beschermde mappen aanpassen, werkt op het hele project).
    purchases_flow: Mutex<std::collections::HashSet<String>>,
}

impl ScopeRegistry {
    pub fn set(&self, project_id: &str, s: RunScope) {
        self.runs.lock().unwrap_or_else(|e| e.into_inner()).insert(project_id.to_string(), s);
    }
    pub fn allow_purchases(&self, project_id: &str) {
        self.purchases_flow.lock().unwrap_or_else(|e| e.into_inner()).insert(project_id.to_string());
    }
    pub fn take_purchases(&self, project_id: &str) -> bool {
        self.purchases_flow.lock().unwrap_or_else(|e| e.into_inner()).remove(project_id)
    }
    pub fn set_selection(&self, project_id: &str, ranges: Vec<(String, usize, usize)>) {
        let mut m = self.selections.lock().unwrap_or_else(|e| e.into_inner());
        if ranges.is_empty() {
            m.remove(project_id);
        } else {
            m.insert(project_id.to_string(), ranges);
        }
    }
    pub fn take_selection(&self, project_id: &str) -> Option<Vec<(String, usize, usize)>> {
        self.selections.lock().unwrap_or_else(|e| e.into_inner()).remove(project_id)
    }
    pub fn take(&self, project_id: &str) -> Option<RunScope> {
        self.runs.lock().unwrap_or_else(|e| e.into_inner()).remove(project_id)
    }
    pub fn with<R>(&self, project_id: &str, f: impl FnOnce(&mut RunScope) -> R) -> Option<R> {
        self.runs.lock().unwrap_or_else(|e| e.into_inner()).get_mut(project_id).map(f)
    }
}

/// Per-run instellingen: bewerkingen gaan via de permissie-tool, zodat Studio ze tegen het werkgebied kan toetsen.
pub fn settings_json(level: Level) -> Option<Value> {
    (level != Level::Project).then(|| serde_json::json!({ "permissions": { "ask": ["Edit", "Write", "MultiEdit", "NotebookEdit"] } }))
}

pub const EDIT_TOOLS: &[&str] = &["Edit", "Write", "MultiEdit", "NotebookEdit"];

#[cfg(test)]
mod tests {
    #[test]
    fn the_purchases_folders_are_closed_to_normal_runs_even_at_project_level() {
        let idx = CodeIndex::default();
        let d = Decision { level: Level::Project, screen: None, why: Why::Fallback };
        let mut s = RunScope::new(&idx, &d);
        assert!(s.permits("lib/features/home/home_screen.dart"));
        assert!(!s.permits("lib/core/purchases/entitlements.dart") && !s.permits("./lib/features/paywall/paywall_screen.dart"));
        s.purchases_ok = true;
        assert!(s.permits("lib/core/purchases/entitlements.dart") && s.permits("lib/features/paywall/paywall_screen.dart"));
        let mut small = RunScope::new(&idx, &Decision { level: Level::Small, screen: None, why: Why::Fallback });
        small.purchases_ok = true;
        assert!(!small.permits("lib/core/purchases/entitlements.dart"), "the flow itself runs at project level; others never get in");
    }

    use super::*;
    use crate::index::fixture;

    fn idx() -> CodeIndex {
        CodeIndex::build(&fixture("scope")).unwrap()
    }
    fn no_ai(_: &str) -> Option<String> {
        None
    }

    #[test]
    fn selection_beats_everything_and_text_sets_the_level() {
        let i = idx();
        let ctx = Some("The user selected the screen \"SettingsScreen\" (lib/features/settings/settings_screen.dart). \"This screen\" means that one.");
        let d = decide(&i, "make the title bigger on home", ctx, Some("HomeScreen"), &mut no_ai);
        assert_eq!((d.level, d.screen.as_deref(), d.why), (Level::Small, Some("SettingsScreen"), Why::Selection));
        let d = decide(&i, "add a search bar with filters and a result list that remembers the last query", ctx, None, &mut no_ai);
        assert_eq!(d.level, Level::Screen);
        let d = decide(&i, "change the theme to dark blue", ctx, None, &mut no_ai);
        assert_eq!(d.level, Level::Shared);
        let d = decide(&i, "add login with email", ctx, None, &mut no_ai);
        assert_eq!(d.level, Level::Project);
    }

    #[test]
    fn current_screen_needs_a_deictic_word_then_keywords_find_the_screen() {
        let i = idx();
        let d = decide(&i, "make this screen greener", None, Some("HomeScreen"), &mut no_ai);
        assert_eq!((d.screen.as_deref(), d.why), (Some("HomeScreen"), Why::Current));
        let d = decide(&i, "Change the text on the settings screen", None, Some("HomeScreen"), &mut no_ai);
        assert_eq!((d.level, d.screen.as_deref(), d.why), (Level::Small, Some("SettingsScreen"), Why::Keyword));
        let d = decide(&i, "pas de home pagina aan", None, None, &mut no_ai);
        assert_eq!(d.screen.as_deref(), Some("HomeScreen"));
    }

    #[test]
    fn asking_for_another_screen_means_shared_work_because_the_router_and_core_change_too() {
        // De evaluatie liet zien dat "a second screen ... icon button ... labelled" als een kleine tweak werd gezien.
        let i = idx();
        let d = decide(&i, "Add a second screen called Settings with a switch labelled 'Sound effects', reachable from an icon button in the top bar of the home screen.", None, None, &mut no_ai);
        assert_eq!(d.level, Level::Shared, "{d:?}");
        assert_eq!(decide(&i, "voeg een tweede scherm toe voor instellingen", None, None, &mut no_ai).level, Level::Shared);
    }

    #[test]
    fn two_screens_in_one_request_means_at_least_shared() {
        let d = decide(&idx(), "use the same header on home and settings", None, None, &mut no_ai);
        assert_eq!((d.level, d.screen), (Level::Shared, None));
    }

    #[test]
    fn ai_is_asked_once_only_when_nothing_else_matches() {
        let i = idx();
        let mut asked = vec![];
        let d = decide(&i, "make it feel friendlier", None, None, &mut |q| {
            asked.push(q.to_string());
            Some("Sure: {\"level\":\"screen\",\"screen\":\"HomeScreen\"}".into())
        });
        assert_eq!((d.level, d.screen.as_deref(), d.why), (Level::Screen, Some("HomeScreen"), Why::Ai));
        assert_eq!(asked.len(), 1);
        assert!(asked[0].contains("HomeScreen, SettingsScreen"));
        // Geen AI-antwoord of onzin: veilige terugval (project), of het huidige scherm als dat bekend is.
        let d = decide(&i, "make it feel friendlier", None, None, &mut no_ai);
        assert_eq!((d.level, d.why), (Level::Project, Why::Fallback));
        let d = decide(&i, "make it feel friendlier", None, Some("SettingsScreen"), &mut |_| Some("no idea".into()));
        assert_eq!((d.screen.as_deref(), d.why), (Some("SettingsScreen"), Why::Fallback));
        // Keyword-treffers kosten nooit een AI-vraag.
        let mut n = 0;
        decide(&i, "change the theme", None, None, &mut |_| {
            n += 1;
            None
        });
        assert_eq!(n, 0);
    }

    #[test]
    fn each_level_allows_exactly_its_files() {
        let i = idx();
        let mk = |level| RunScope::new(&i, &Decision { level, screen: Some("HomeScreen".into()), why: Why::Selection });
        let small = mk(Level::Small);
        assert!(small.permits("lib/features/home/home_screen.dart"));
        assert!(!small.permits("lib/features/home/widgets/card.dart"));
        assert!(small.permits("STUDIO.md") && small.permits(".studio/theme.json"));
        let screen = mk(Level::Screen);
        assert!(screen.permits("lib/features/home/widgets/card.dart") && screen.permits("test/widget_test.dart"));
        assert!(!screen.permits("lib/core/theme/app_theme.dart") && !screen.permits("lib/features/settings/settings_screen.dart"));
        assert!(!screen.permits("pubspec.yaml"));
        let shared = mk(Level::Shared);
        assert!(shared.permits("lib/core/theme/app_theme.dart") && shared.permits("pubspec.yaml"));
        assert!(!shared.permits("lib/features/settings/settings_screen.dart"));
        assert!(mk(Level::Project).permits("lib/features/settings/settings_screen.dart"));
    }

    #[test]
    fn paths_are_checked_inside_the_project_only() {
        let i = idx();
        let s = RunScope::new(&i, &Decision { level: Level::Screen, screen: Some("HomeScreen".into()), why: Why::Selection });
        let dir = Path::new("/work/demo");
        assert!(s.permits_path(dir, "/work/demo/lib/features/home/home_screen.dart"));
        assert!(!s.permits_path(dir, "/work/other/lib/features/home/home_screen.dart"));
        assert!(!s.permits_path(dir, "lib/features/home/../../core/theme/app_theme.dart"));
        assert!(s.permits_path(dir, "lib/features/home/home_screen.dart"));
    }

    #[test]
    fn expansion_goes_one_level_up_or_adds_specific_files_and_is_logged() {
        let i = idx();
        let mut s = RunScope::new(&i, &Decision { level: Level::Small, screen: Some("HomeScreen".into()), why: Why::Keyword });
        s.expand(&i, None, &[], "needs the card widget");
        assert_eq!(s.level, Level::Screen);
        assert!(s.permits("lib/features/home/widgets/card.dart"));
        s.expand(&i, None, &["lib/core/theme/app_theme.dart".into()], "needs one token");
        assert_eq!(s.level, Level::Screen, "specific files do not raise the level");
        assert!(s.permits("lib/core/theme/app_theme.dart") && !s.permits("lib/core/widgets/empty_state.dart"));
        s.expand(&i, Some(Level::Project), &[], "restructure");
        assert!(s.permits("anything.dart"));
        assert_eq!(s.expansions.len(), 3);
        s.expand(&i, Some(Level::Small), &[], "cannot shrink");
        assert_eq!(s.level, Level::Project);
        assert!(s.describe().contains("anything"));
    }

    #[test]
    fn describe_names_the_allowed_files_and_the_escape_hatch() {
        let i = idx();
        let s = RunScope::new(&i, &Decision { level: Level::Screen, screen: Some("HomeScreen".into()), why: Why::Selection });
        let d = s.describe();
        assert!(d.contains("lib/features/home/**") && d.contains("breid_bereik_uit") && d.contains("HomeScreen"));
        assert!(settings_json(Level::Project).is_none());
        assert_eq!(settings_json(Level::Screen).unwrap()["permissions"]["ask"][0], "Edit");
    }

    #[test]
    fn file_paths_in_the_request_do_not_change_the_level() {
        assert_eq!(level_from_text("Add a comment to lib/core/models/player.dart and lib/core/theme/tokens.dart", true), Some(Level::Screen));
        assert_eq!(level_from_text("change the theme", true), Some(Level::Shared));
    }

    #[test]
    fn breaches_find_edits_outside_the_scope_but_ignore_tool_output() {
        let i = idx();
        let mut s = RunScope::new(&i, &Decision { level: Level::Screen, screen: Some("HomeScreen".into()), why: Why::Selection });
        let changed: Vec<String> = ["lib/features/home/home_screen.dart", "lib/core/theme/app_theme.dart", "pubspec.lock", "ios/Podfile.lock", "STUDIO.md", "test/a_test.dart", "lib/features/settings/settings_screen.dart"].map(String::from).to_vec();
        assert_eq!(s.breaches(&changed), vec!["lib/core/theme/app_theme.dart", "lib/features/settings/settings_screen.dart"]);
        s.expand(&i, None, &["lib/core/theme/app_theme.dart".into()], "x");
        assert_eq!(s.breaches(&changed), vec!["lib/features/settings/settings_screen.dart"]);
    }

    #[test]
    fn registry_hands_the_scope_to_whoever_needs_it() {
        let i = idx();
        let r = ScopeRegistry::default();
        r.set("p", RunScope::new(&i, &Decision { level: Level::Screen, screen: Some("HomeScreen".into()), why: Why::Selection }));
        assert_eq!(r.with("p", |s| s.level), Some(Level::Screen));
        assert!(r.with("other", |s| s.level).is_none());
        assert!(r.take("p").is_some() && r.take("p").is_none());
    }
}

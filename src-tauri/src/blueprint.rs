//! Blueprint van een nieuwe app (`studio/blueprint.json`): de bron van waarheid voor het bouwen. Streng gevalideerd, zonder geheimen,
//! binnen de catalogus van ondersteunde functies (`studio/capabilities.json`). De AI levert hem als gestructureerde uitvoer; Studio valideert
//! en laat hem één keer herstellen als hij afwijkt.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const CAPABILITIES: &str = include_str!("../capabilities.json");
pub const MAX_PAGES: usize = 12;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Blueprint {
    pub version: u32,
    pub app: AppInfo,
    pub style: Style,
    pub navigation: Navigation,
    pub pages: Vec<Page>,
    pub models: Vec<Model>,
    pub features: Vec<Feature>,
    pub integrations: Integrations,
    pub assumptions: Vec<Assumption>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct AppInfo {
    pub name: String,
    pub purpose: String,
    pub audience: String,
    pub language: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Style {
    /// calm | lively | sleek
    pub card: String,
    pub seed: String,
    pub font: String,
    pub kit: String,
    pub logo: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Navigation {
    /// tabs | menu | single
    pub kind: String,
    pub order: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Page {
    pub id: String,
    pub name: String,
    pub purpose: String,
    pub route: String,
    pub components: Vec<String>,
    pub models: Vec<String>,
    pub features: Vec<String>,
    pub links: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Model {
    pub id: String,
    pub name: String,
    pub fields: Vec<Field>,
    pub sample: Vec<serde_json::Map<String, serde_json::Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Field {
    pub name: String,
    /// text | int | double | bool | date | enum | ref
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(rename = "ref")]
    pub reference: Option<String>,
    pub values: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Feature {
    pub id: String,
    pub title: String,
    pub capability: String,
    pub pages: Vec<String>,
    /// now | later
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Integrations {
    /// local | supabase | firebase
    pub storage: String,
    pub auth: Vec<String>,
    pub purchases: bool,
    pub notifications: bool,
}
impl Default for Integrations {
    fn default() -> Self {
        Integrations { storage: "local".into(), auth: vec![], purchases: false, notifications: false }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Assumption {
    pub id: String,
    pub text: String,
    pub editable: bool,
}

// ───────────── catalogus ─────────────

#[derive(Debug, Clone, Deserialize)]
pub struct CapItem {
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub alternative: String,
    #[serde(default)]
    pub alternative_id: String,
    #[serde(default)]
    pub needs: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Capabilities {
    pub supported: Vec<CapItem>,
    pub optional: Vec<CapItem>,
    pub unsupported: Vec<CapItem>,
}

pub fn capabilities() -> Capabilities {
    serde_json::from_str(CAPABILITIES).expect("capabilities.json is valid")
}

impl Capabilities {
    pub fn is_known(&self, id: &str) -> bool {
        self.supported.iter().chain(&self.optional).any(|c| c.id == id)
    }
    pub fn needs(&self, id: &str) -> Option<&serde_json::Value> {
        self.optional.iter().find(|c| c.id == id).map(|c| &c.needs)
    }
}

/// Functies uit een vrije tekst die Studio niet ondersteunt: (id, titel, alternatief). De UI legt dat in gewone taal uit bij de stap Functies.
pub fn unsupported_in(text: &str, caps: &Capabilities) -> Vec<(String, String, String)> {
    let lower = text.to_lowercase();
    caps.unsupported.iter().filter(|u| u.keywords.iter().any(|k| lower.contains(k.as_str()))).map(|u| (u.id.clone(), u.title.clone(), u.alternative.clone())).collect()
}

// ───────────── validatie ─────────────

fn slug_ok(s: &str) -> bool {
    !s.is_empty() && s.len() <= 30 && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') && s.chars().next().is_some_and(|c| c.is_ascii_lowercase())
}
fn camel_ok(s: &str) -> bool {
    !s.is_empty() && s.len() <= 40 && s.chars().next().is_some_and(|c| c.is_ascii_lowercase()) && s.chars().all(|c| c.is_ascii_alphanumeric())
}
const SECRET_WORDS: [&str; 5] = ["password", "wachtwoord", "token", "secret", "apikey"];

/// Alle afwijkingen van het schema, in gewone tekst (ook de invoer voor de herstelronde van de AI).
pub fn validate(b: &Blueprint, caps: &Capabilities) -> Vec<String> {
    let mut v = vec![];
    if b.version != 1 {
        v.push("version must be 1".into());
    }
    let name_len = b.app.name.chars().count();
    if name_len == 0 || name_len > 30 {
        v.push("app.name must be 1-30 characters".into());
    }
    if b.app.purpose.chars().count() > 300 || b.app.audience.chars().count() > 200 || b.app.description.chars().count() > 4000 {
        v.push("app.purpose (300), app.audience (200) or app.description (4000) is too long".into());
    }
    if b.app.language.trim().is_empty() {
        v.push("app.language is required (for example nl or en)".into());
    }
    if !crate::studio_tools::valid_hex(&b.style.seed) {
        v.push("style.seed must look like #4F46E5".into());
    }
    if !crate::config::FONTS.contains(&b.style.font.as_str()) {
        v.push(format!("style.font must be one of {}", crate::config::FONTS.join(", ")));
    }
    if !crate::config::KITS.contains(&b.style.kit.as_str()) {
        v.push(format!("style.kit must be one of {}", crate::config::KITS.join(", ")));
    }
    if !["calm", "lively", "sleek"].contains(&b.style.card.as_str()) {
        v.push("style.card must be calm, lively or sleek".into());
    }
    if b.pages.is_empty() || b.pages.len() > MAX_PAGES {
        v.push(format!("there must be 1-{MAX_PAGES} pages"));
    }
    let mut ids = BTreeSet::new();
    let mut routes = BTreeSet::new();
    for p in &b.pages {
        if !slug_ok(&p.id) {
            v.push(format!("page id '{}' must be lowercase letters, digits or _ (max 30)", p.id));
        }
        if !ids.insert(p.id.clone()) {
            v.push(format!("duplicate page id '{}'", p.id));
        }
        if p.name.trim().is_empty() || p.name.chars().count() > 30 {
            v.push(format!("page '{}' needs a name of 1-30 characters", p.id));
        }
        if !p.route.starts_with('/') || p.route.len() > 60 || !p.route.chars().all(|c| c.is_ascii_alphanumeric() || "/_-:".contains(c)) {
            v.push(format!("page '{}' has an invalid route '{}'", p.id, p.route));
        }
        if !routes.insert(p.route.clone()) {
            v.push(format!("duplicate route '{}'", p.route));
        }
    }
    if !b.pages.iter().any(|p| p.route == "/") {
        v.push("one page must have the route '/' (the start page)".into());
    }
    let page_ids: BTreeSet<&str> = b.pages.iter().map(|p| p.id.as_str()).collect();
    let model_ids: BTreeSet<&str> = b.models.iter().map(|m| m.id.as_str()).collect();
    let feature_ids: BTreeSet<&str> = b.features.iter().map(|f| f.id.as_str()).collect();
    if !["tabs", "menu", "single"].contains(&b.navigation.kind.as_str()) {
        v.push("navigation.kind must be tabs, menu or single".into());
    }
    for id in &b.navigation.order {
        if !page_ids.contains(id.as_str()) {
            v.push(format!("navigation.order refers to an unknown page '{id}'"));
        }
    }
    for p in &b.pages {
        for l in &p.links {
            if !page_ids.contains(l.as_str()) {
                v.push(format!("page '{}' links to an unknown page '{l}'", p.id));
            }
        }
        for m in &p.models {
            if !model_ids.contains(m.as_str()) {
                v.push(format!("page '{}' uses an unknown model '{m}'", p.id));
            }
        }
        for f in &p.features {
            if !feature_ids.contains(f.as_str()) {
                v.push(format!("page '{}' uses an unknown feature '{f}'", p.id));
            }
        }
    }
    let mut mids = BTreeSet::new();
    for m in &b.models {
        if !slug_ok(&m.id) || !mids.insert(m.id.clone()) {
            v.push(format!("model id '{}' must be unique lowercase letters, digits or _", m.id));
        }
        if m.fields.is_empty() {
            v.push(format!("model '{}' needs at least one field", m.id));
        }
        for f in &m.fields {
            if !camel_ok(&f.name) {
                v.push(format!("field '{}' of model '{}' must be camelCase letters and digits", f.name, m.id));
            }
            if SECRET_WORDS.iter().any(|w| f.name.to_lowercase().contains(w)) {
                v.push(format!("field '{}' of model '{}' looks like a secret; never store secrets in the data model", f.name, m.id));
            }
            if !["text", "int", "double", "bool", "date", "enum", "ref"].contains(&f.kind.as_str()) {
                v.push(format!("field '{}' of model '{}' has an unknown type '{}'", f.name, m.id, f.kind));
            }
            if f.kind == "ref" && !f.reference.as_deref().is_some_and(|r| model_ids.contains(r)) {
                v.push(format!("field '{}' of model '{}' refers to an unknown model", f.name, m.id));
            }
            if f.kind == "enum" && f.values.is_empty() {
                v.push(format!("enum field '{}' of model '{}' needs values", f.name, m.id));
            }
        }
        let names: BTreeSet<&str> = m.fields.iter().map(|f| f.name.as_str()).collect();
        for (i, row) in m.sample.iter().enumerate() {
            for k in row.keys() {
                if !names.contains(k.as_str()) {
                    v.push(format!("sample row {i} of model '{}' has an unknown field '{k}'", m.id));
                }
            }
        }
    }
    let mut fids = BTreeSet::new();
    for f in &b.features {
        if !slug_ok(&f.id) || !fids.insert(f.id.clone()) {
            v.push(format!("feature id '{}' must be unique lowercase letters, digits or _", f.id));
        }
        if !caps.is_known(&f.capability) {
            let unsupported = caps.unsupported.iter().any(|u| u.id == f.capability);
            v.push(if unsupported {
                format!("feature '{}' uses '{}', which Studio does not support; use the alternative instead", f.id, f.capability)
            } else {
                format!("feature '{}' uses an unknown capability '{}'", f.id, f.capability)
            });
        }
        if !["now", "later"].contains(&f.status.as_str()) {
            v.push(format!("feature '{}' status must be now or later", f.id));
        }
        for p in &f.pages {
            if !page_ids.contains(p.as_str()) {
                v.push(format!("feature '{}' refers to an unknown page '{p}'", f.id));
            }
        }
        // een optionele functie schakelt koppelingen in; die moeten dan ook echt aan staan
        if let Some(needs) = caps.needs(&f.capability) {
            if f.status == "now" {
                let i = &b.integrations;
                let ok = match (needs["auth"].as_str(), needs["storage"].as_str(), needs["notifications"].as_bool(), needs["purchases"].as_bool()) {
                    (Some(a), _, _, _) => i.auth.iter().any(|x| x == a),
                    (_, Some(s), _, _) => i.storage == s,
                    (_, _, Some(true), _) => i.notifications,
                    (_, _, _, Some(true)) => i.purchases,
                    _ => true,
                };
                if !ok {
                    v.push(format!("feature '{}' needs integrations that are switched off", f.id));
                }
            }
        }
    }
    let i = &b.integrations;
    if !["local", "supabase", "firebase"].contains(&i.storage.as_str()) {
        v.push("integrations.storage must be local, supabase or firebase".into());
    }
    if i.auth.iter().any(|a| !["email", "anonymous"].contains(&a.as_str())) {
        v.push("integrations.auth may only contain email or anonymous".into());
    }
    // Lokaal en zonder account is de standaard: iets anders alleen als een functie het echt vraagt.
    let needs_cloud = b.features.iter().any(|f| f.status == "now" && caps.needs(&f.capability).is_some_and(|n| n["storage"].is_string()));
    if i.storage != "local" && !needs_cloud {
        v.push("integrations.storage must stay local unless a feature needs cloud sync".into());
    }
    let needs_auth = b.features.iter().any(|f| f.status == "now" && caps.needs(&f.capability).is_some_and(|n| n["auth"].is_string()));
    if !i.auth.is_empty() && !needs_auth {
        v.push("integrations.auth must stay empty unless a feature needs sign-in".into());
    }
    let mut aids = BTreeSet::new();
    for a in &b.assumptions {
        if !aids.insert(a.id.clone()) || a.text.trim().is_empty() || a.text.chars().count() > 200 {
            v.push(format!("assumption '{}' needs a unique id and a text of at most 200 characters", a.id));
        }
    }
    v
}

// ───────────── uit AI-tekst halen en laten herstellen ─────────────

/// Eerste JSON-object in een tekst (ook binnen ``` hekken); de AI kan er uitleg omheen zetten.
pub fn extract_json(text: &str) -> Option<&str> {
    let start = text.find('{')?;
    let mut depth = 0i32;
    let mut in_str = false;
    let mut esc = false;
    for (i, c) in text[start..].char_indices() {
        if in_str {
            if esc {
                esc = false;
            } else if c == '\\' {
                esc = true;
            } else if c == '"' {
                in_str = false;
            }
            continue;
        }
        match c {
            '"' => in_str = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&text[start..start + i + 1]);
                }
            }
            _ => {}
        }
    }
    None
}

#[derive(Debug, PartialEq)]
pub enum PlanError {
    /// De AI gaf niets bruikbaars (ook niet na de herstelronde).
    NoAnswer,
    /// Nog afwijkingen na één herstelronde: het plan zoals het is en wat er nog mis is.
    Invalid(Box<Blueprint>, Vec<String>),
}

/// Vraagt de AI om een blueprint en herstelt één keer bij afwijkingen (dezelfde vraag met de foutenlijst). `ask` geeft de tekst van het antwoord.
pub fn plan(ask: &mut dyn FnMut(&str) -> Option<String>, prompt: &str, caps: &Capabilities) -> Result<Blueprint, PlanError> {
    let parse = |t: &str| extract_json(t).and_then(|j| serde_json::from_str::<Blueprint>(j).ok());
    let first = ask(prompt).and_then(|t| parse(&t));
    let Some(mut bp) = first else {
        // geen leesbaar JSON-object: één herhaling met een duidelijke eis
        let again = ask(&format!("{prompt}\n\nYour last answer was not a valid JSON object. Answer with ONLY the JSON object."));
        return match again.and_then(|t| parse(&t)) {
            Some(bp) => finish(bp, caps),
            None => Err(PlanError::NoAnswer),
        };
    };
    let issues = validate(&bp, caps);
    if issues.is_empty() {
        return Ok(bp);
    }
    let fix = format!("{prompt}\n\nYour blueprint had these problems. Return the corrected complete JSON object (only the JSON):\n- {}\n\nYour blueprint:\n{}", issues.join("\n- "), serde_json::to_string(&bp).unwrap_or_default());
    if let Some(better) = ask(&fix).and_then(|t| parse(&t)) {
        bp = better;
    }
    finish(bp, caps)
}

fn finish(bp: Blueprint, caps: &Capabilities) -> Result<Blueprint, PlanError> {
    let issues = validate(&bp, caps);
    if issues.is_empty() { Ok(bp) } else { Err(PlanError::Invalid(Box::new(bp), issues)) }
}

/// De prompt voor de planning: de beschrijving, gekozen functies en stijl, en de vaste regels (catalogus, lokaal standaard, aannames, geen vragen).
pub fn plan_prompt(description: &str, features: &[String], style_hint: Option<&str>, language: &str) -> String {
    let caps = capabilities();
    let list = |v: &Vec<CapItem>| v.iter().map(|c| c.id.as_str()).collect::<Vec<_>>().join(", ");
    format!(
        "You are planning a new mobile app (Flutter). Answer with ONLY one JSON object, no explanation.\n\
         App description from the user (any language): {description}\n\
         Features the user wants (may be empty, then propose sensible ones): {}\n\
         {}\
         Rules:\n\
         - Do NOT ask questions. If something is unclear, make a sensible choice and write it down as an assumption (one short line each).\n\
         - Default to data stored on the phone (integrations.storage \"local\", auth []). Only use cloud sync or sign-in when a feature truly needs it.\n\
         - Every feature uses one capability from: {} (optional: {}). Never use unsupported ideas ({}); choose a feasible alternative instead.\n\
         - 1 to {MAX_PAGES} pages; one page has route \"/\". Page ids are lowercase_with_underscores. Include sample data (3-6 rows per model) so every page shows something useful at once.\n\
         - Write names and texts for the user in the language of the description (app.language = \"{language}\" unless the description is clearly another language).\n\
         - Never put secrets in the plan.\n\
         JSON schema (version 1): {{\"version\":1,\"app\":{{\"name\",\"purpose\",\"audience\",\"language\",\"description\"}},\"style\":{{\"card\":\"calm|lively|sleek\",\"seed\":\"#RRGGBB\",\"font\":\"Inter|Poppins|Lora|Roboto\",\"kit\":\"material|cupertino|rounded|minimal\",\"logo\":null}},\
         \"navigation\":{{\"kind\":\"tabs|menu|single\",\"order\":[pageIds]}},\"pages\":[{{\"id\",\"name\",\"purpose\",\"route\",\"components\":[],\"models\":[],\"features\":[],\"links\":[]}}],\
         \"models\":[{{\"id\",\"name\",\"fields\":[{{\"name\",\"type\":\"text|int|double|bool|date|enum|ref\",\"ref\":null,\"values\":[]}}],\"sample\":[{{}}]}}],\
         \"features\":[{{\"id\",\"title\",\"capability\",\"pages\":[],\"status\":\"now|later\"}}],\"integrations\":{{\"storage\":\"local\",\"auth\":[],\"purchases\":false,\"notifications\":false}},\
         \"assumptions\":[{{\"id\",\"text\",\"editable\":true}}]}}",
        if features.is_empty() { "-".to_string() } else { features.join("; ") },
        style_hint.map(|s| format!("Preferred style: {s}\n")).unwrap_or_default(),
        list(&caps.supported),
        list(&caps.optional),
        caps.unsupported.iter().map(|u| u.id.as_str()).collect::<Vec<_>>().join(", "),
    )
}

#[cfg(test)]
pub fn sample() -> Blueprint {
    let row = |p: &str, k: i64| {
        let mut m = serde_json::Map::new();
        m.insert("name".into(), p.into());
        m.insert("points".into(), k.into());
        m
    };
    Blueprint {
        version: 1,
        app: AppInfo { name: "Friday Scores".into(), purpose: "Keep score on game night".into(), audience: "Friends".into(), language: "en".into(), description: "A scoreboard for friday games".into() },
        style: Style { card: "calm".into(), seed: "#0D9488".into(), font: "Inter".into(), kit: "rounded".into(), logo: None },
        navigation: Navigation { kind: "tabs".into(), order: vec!["home".into(), "players".into(), "settings".into()] },
        pages: vec![
            Page { id: "home".into(), name: "Scoreboard".into(), purpose: "Overview and ranking".into(), route: "/".into(), components: vec!["ranking list".into()], models: vec!["player".into()], features: vec!["ranking".into()], links: vec!["players".into()] },
            Page { id: "players".into(), name: "Players".into(), purpose: "Manage players".into(), route: "/players".into(), components: vec!["list".into(), "add form".into()], models: vec!["player".into()], features: vec!["players".into()], links: vec![] },
            Page { id: "settings".into(), name: "Settings".into(), purpose: "Name and theme".into(), route: "/settings".into(), components: vec!["theme choice".into()], models: vec![], features: vec![], links: vec![] },
        ],
        models: vec![Model {
            id: "player".into(),
            name: "Player".into(),
            fields: vec![Field { name: "name".into(), kind: "text".into(), reference: None, values: vec![] }, Field { name: "points".into(), kind: "int".into(), reference: None, values: vec![] }],
            sample: vec![row("Anna", 12), row("Bram", 9), row("Cleo", 15)],
        }],
        features: vec![
            Feature { id: "ranking".into(), title: "Season ranking".into(), capability: "lists".into(), pages: vec!["home".into()], status: "now".into() },
            Feature { id: "players".into(), title: "Add and remove players".into(), capability: "forms".into(), pages: vec!["players".into()], status: "now".into() },
        ],
        integrations: Integrations::default(),
        assumptions: vec![Assumption { id: "local".into(), text: "I assumed everything is stored on the phone, no account.".into(), editable: true }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_good_blueprint_validates() {
        assert_eq!(validate(&sample(), &capabilities()), Vec::<String>::new());
    }

    #[test]
    fn every_kind_of_deviation_is_reported_in_plain_text() {
        let caps = capabilities();
        let mut b = sample();
        b.pages[1].id = "Bad Id".into();
        b.pages[2].route = "/".into();
        b.navigation.order.push("ghost".into());
        b.features[0].capability = "realtime_multiplayer".into();
        b.features[1].capability = "teleport".into();
        b.models[0].fields.push(Field { name: "password".into(), kind: "text".into(), reference: None, values: vec![] });
        b.models[0].sample[0].insert("nope".into(), 1.into());
        b.integrations.storage = "supabase".into();
        b.style.seed = "teal".into();
        let issues = validate(&b, &caps).join("\n");
        for needle in ["Bad Id", "duplicate route", "unknown page 'ghost'", "does not support", "unknown capability 'teleport'", "looks like a secret", "unknown field 'nope'", "stay local", "style.seed"] {
            assert!(issues.contains(needle), "missing {needle} in:\n{issues}");
        }
    }

    #[test]
    fn cloud_or_sign_in_is_only_allowed_when_a_feature_needs_it() {
        let caps = capabilities();
        let mut b = sample();
        b.features.push(Feature { id: "sync".into(), title: "Sync".into(), capability: "cloud_sync_supabase".into(), pages: vec![], status: "now".into() });
        assert!(validate(&b, &caps).iter().any(|i| i.contains("needs integrations that are switched off")));
        b.integrations.storage = "supabase".into();
        assert!(validate(&b, &caps).is_empty());
        b.features[2].status = "later".into();
        assert!(validate(&b, &caps).iter().any(|i| i.contains("stay local")), "a 'later' feature does not switch the cloud on");
    }

    #[test]
    fn unsupported_ideas_in_free_text_are_found_with_an_alternative() {
        let caps = capabilities();
        let found = unsupported_in("Een spel met live meespelen op meerdere telefoons en een 3d kaart", &caps);
        let ids: Vec<&str> = found.iter().map(|f| f.0.as_str()).collect();
        assert_eq!(ids, vec!["realtime_multiplayer", "three_d"]);
        assert!(found[0].2.contains("One phone"));
        assert!(unsupported_in("A shopping list", &caps).is_empty());
    }

    #[test]
    fn json_is_found_inside_text_and_code_fences() {
        let t = "Sure!\n```json\n{\"a\": {\"b\": \"}\"}, \"c\": 1}\n```\nDone.";
        assert_eq!(extract_json(t), Some("{\"a\": {\"b\": \"}\"}, \"c\": 1}"));
        assert_eq!(extract_json("no json"), None);
    }

    #[test]
    fn a_deviating_plan_is_repaired_once_and_a_second_failure_is_reported() {
        let caps = capabilities();
        let good = serde_json::to_string(&sample()).unwrap();
        let mut bad = sample();
        bad.pages[0].route = "home".into();
        let bad_s = serde_json::to_string(&bad).unwrap();
        // eerst afwijkend, dan hersteld
        let mut calls = 0;
        let ok = plan(&mut |q| { calls += 1; Some(if calls == 1 { format!("Here: {bad_s}") } else { assert!(q.contains("invalid route")); good.clone() }) }, "P", &caps).unwrap();
        assert_eq!((calls, ok.pages.len()), (2, 3));
        // blijft afwijkend: precies één herstelronde en het plan komt terug met de problemen
        let mut calls = 0;
        let err = plan(&mut |_| { calls += 1; Some(bad_s.clone()) }, "P", &caps).unwrap_err();
        assert_eq!(calls, 2);
        assert!(matches!(err, PlanError::Invalid(_, ref v) if !v.is_empty()));
        // geen JSON: één herhaling, dan NoAnswer
        let mut calls = 0;
        assert_eq!(plan(&mut |_| { calls += 1; Some("sorry".into()) }, "P", &caps).unwrap_err(), PlanError::NoAnswer);
        assert_eq!(calls, 2);
    }

    #[test]
    fn the_planning_prompt_carries_the_rules_and_never_asks_questions() {
        let p = plan_prompt("scorebord", &["spelers".into()], Some("calm teal"), "nl");
        for needle in ["Do NOT ask questions", "local", "unsupported", "JSON schema", "scorebord", "spelers", "calm teal"] {
            assert!(p.contains(needle), "{needle}");
        }
    }
}

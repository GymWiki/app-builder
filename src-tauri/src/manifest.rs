//! Integratie-manifesten (fase 10a): één JSON-bestand per integratie in `integrations/`. Alle logica van de pagina's
//! Integraties en Geheimen komt uit deze data. Teksten (`naam`, `label`, `hulptekst`, …) zijn i18n-sleutels die de UI
//! vertaalt, zodat Engels en Nederlands altijd gelijk lopen.

use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    Global,
    Project,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FieldKind {
    Text,
    Password,
    File,
    Select,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Validation {
    pub regex: Option<String>,
    pub bestandstype: Option<Vec<String>>,
    #[serde(rename = "minLengte")]
    pub min_lengte: Option<usize>,
    /// Tekst die in een bestand moet staan (bv. "PRIVATE KEY" in een .p8).
    pub bevat: Option<String>,
    /// JSON-bestand: paden die moeten bestaan, of `pad=waarde`.
    pub json: Option<Vec<String>>,
    /// Een sleutel in een publiek veld mag niet de geheime (service_role) sleutel zijn.
    #[serde(rename = "geenServiceRol")]
    pub geen_service_rol: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Field {
    pub id: String,
    pub label: String,
    #[serde(rename = "type")]
    pub kind: FieldKind,
    pub geheim: bool,
    pub verplicht: bool,
    pub scope: Scope,
    #[serde(default)]
    pub validatie: Validation,
    pub hulptekst: String,
    #[serde(rename = "guideId", default)]
    pub guide_id: Option<String>,
    /// Naam van de omgevingsvariabele waarmee Studio de waarde aan een kortlevend subproces meegeeft.
    #[serde(rename = "envVar", default)]
    pub env_var: Option<String>,
    /// Niet-geheime waarden die uit een bestand worden gelezen (JSON-paden), bv. `client_email`.
    #[serde(default)]
    pub afgeleid: Vec<String>,
    #[serde(default)]
    pub standaard: Option<String>,
    #[serde(default)]
    pub opties: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Manifest {
    pub id: String,
    pub naam: String,
    pub categorie: String,
    pub beschrijving: String,
    pub icoon: String,
    pub scope: Scope,
    pub velden: Vec<Field>,
    #[serde(rename = "testVerbinding", default)]
    pub test_verbinding: Option<String>,
    #[serde(rename = "checklistItems", default)]
    pub checklist_items: Vec<String>,
    #[serde(default)]
    pub afhankelijkheden: Vec<String>,
    #[serde(default)]
    pub acties: Vec<String>,
}

const SOURCES: &[&str] = &[
    include_str!("../../integrations/apple.json"),
    include_str!("../../integrations/google_play.json"),
    include_str!("../../integrations/supabase.json"),
    include_str!("../../integrations/firebase.json"),
    include_str!("../../integrations/local_db.json"),
    include_str!("../../integrations/github.json"),
    include_str!("../../integrations/vercel.json"),
    include_str!("../../integrations/purchases.json"),
    include_str!("../../integrations/revenuecat.json"),
];

pub fn parse(src: &str) -> AppResult<Manifest> {
    serde_json::from_str(src).map_err(|e| AppError::Invalid(format!("integration manifest: {e}")))
}

pub fn all() -> &'static [Manifest] {
    static ALL: OnceLock<Vec<Manifest>> = OnceLock::new();
    ALL.get_or_init(|| SOURCES.iter().map(|s| parse(s).expect("a built-in integration manifest is valid")).collect())
}

pub fn get(id: &str) -> Option<&'static Manifest> {
    all().iter().find(|m| m.id == id)
}

impl Manifest {
    pub fn field(&self, id: &str) -> Option<&Field> {
        self.velden.iter().find(|f| f.id == id)
    }
}

/// Checklist-sleutels waar een manifest naar mag verwijzen.
pub const CHECKLIST_KEYS: &[&str] = &["app_basics", "data_auth", "legal", "ai_test_run", "store_listing", "accounts_signing", "version_changelog", "final_confirmation", "no_leaks"];

/// Structuurcontrole van een manifest (ook gebruikt in de tests voor de ingebouwde manifesten).
pub fn check(m: &Manifest, known_ids: &[&str]) -> Result<(), String> {
    let mut seen = std::collections::HashSet::new();
    if m.id.is_empty() || !m.id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') {
        return Err(format!("{}: invalid id", m.id));
    }
    for f in &m.velden {
        if !seen.insert(&f.id) {
            return Err(format!("{}.{}: duplicate field", m.id, f.id));
        }
        if f.kind == FieldKind::Select && f.opties.is_empty() {
            return Err(format!("{}.{}: select without options", m.id, f.id));
        }
        if f.kind == FieldKind::Password && !f.geheim {
            return Err(format!("{}.{}: a password field must be secret", m.id, f.id));
        }
        if m.scope == Scope::Project && f.scope == Scope::Global {
            return Err(format!("{}.{}: a global field in a project integration", m.id, f.id));
        }
        if let Some(r) = &f.validatie.regex {
            regex_lite::Regex::new(r).map_err(|e| format!("{}.{}: bad regex: {e}", m.id, f.id))?;
        }
        if f.kind != FieldKind::File && (f.validatie.bestandstype.is_some() || f.validatie.json.is_some()) {
            return Err(format!("{}.{}: file rules on a non-file field", m.id, f.id));
        }
    }
    for d in &m.afhankelijkheden {
        if !known_ids.contains(&d.as_str()) || d == &m.id {
            return Err(format!("{}: unknown dependency {d}", m.id));
        }
    }
    for c in &m.checklist_items {
        if !CHECKLIST_KEYS.contains(&c.as_str()) {
            return Err(format!("{}: unknown checklist item {c}", m.id));
        }
    }
    Ok(())
}

// ───────────── validatie van waarden ─────────────

/// Een afwijzing met een code die de UI vertaalt (met een volgende stap erbij).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Rejection {
    pub code: &'static str,
    pub detail: Option<String>,
}

fn rej(code: &'static str) -> Rejection {
    Rejection { code, detail: None }
}

/// Decodeert het middelste deel van een JWT (alleen om de rol te lezen; de handtekening wordt niet gecontroleerd).
pub fn jwt_role(token: &str) -> Option<String> {
    use base64::Engine;
    let payload = token.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(payload.trim_end_matches('=')).ok()?;
    serde_json::from_slice::<serde_json::Value>(&bytes).ok()?["role"].as_str().map(String::from)
}

pub fn validate_text(f: &Field, value: &str) -> Result<(), Rejection> {
    if f.kind == FieldKind::File {
        return Err(rej("not_text"));
    }
    let v = value.trim();
    if v.is_empty() {
        return Err(rej("empty"));
    }
    if v.contains(['\n', '\r']) && f.kind != FieldKind::Select {
        return Err(rej("multiline"));
    }
    if let Some(min) = f.validatie.min_lengte {
        if v.chars().count() < min {
            return Err(Rejection { code: "too_short", detail: Some(min.to_string()) });
        }
    }
    if f.kind == FieldKind::Select && !f.opties.iter().any(|o| o == v) {
        return Err(rej("not_an_option"));
    }
    if let Some(re) = &f.validatie.regex {
        if !regex_lite::Regex::new(re).map(|r| r.is_match(v)).unwrap_or(false) {
            return Err(rej("pattern"));
        }
    }
    if f.validatie.geen_service_rol == Some(true) && (jwt_role(v).as_deref() == Some("service_role") || v.starts_with("sb_secret_")) {
        return Err(rej("secret_in_public_field"));
    }
    Ok(())
}

fn json_path<'a>(v: &'a serde_json::Value, path: &str) -> Option<&'a serde_json::Value> {
    path.split('.').try_fold(v, |cur, key| cur.get(key))
}

/// Controleert een bestand en geeft de niet-geheime afgeleide waarden terug (bv. `client_email`).
pub fn validate_file(f: &Field, file_name: &str, bytes: &[u8]) -> Result<BTreeMap<String, String>, Rejection> {
    if f.kind != FieldKind::File {
        return Err(rej("not_a_file"));
    }
    if bytes.is_empty() {
        return Err(rej("empty"));
    }
    if bytes.len() > 512 * 1024 {
        return Err(rej("too_large"));
    }
    if let Some(types) = &f.validatie.bestandstype {
        let ext = file_name.rsplit_once('.').map(|(_, e)| e.to_lowercase()).unwrap_or_default();
        if !types.contains(&ext) {
            return Err(Rejection { code: "file_type", detail: Some(types.join(", ")) });
        }
    }
    let mut derived = BTreeMap::new();
    if f.validatie.bevat.is_some() || f.validatie.json.is_some() {
        let text = std::str::from_utf8(bytes).map_err(|_| rej("not_text"))?;
        if let Some(needle) = &f.validatie.bevat {
            if !text.contains(needle.as_str()) {
                return Err(Rejection { code: "content", detail: Some(needle.clone()) });
            }
        }
        if let Some(rules) = &f.validatie.json {
            let v: serde_json::Value = serde_json::from_str(text).map_err(|_| rej("json_invalid"))?;
            for r in rules {
                let (path, want) = r.split_once('=').map(|(p, w)| (p, Some(w))).unwrap_or((r.as_str(), None));
                match (json_path(&v, path), want) {
                    (None, _) => return Err(Rejection { code: "json_missing", detail: Some(path.to_string()) }),
                    (Some(x), Some(w)) if x.as_str() != Some(w) => return Err(Rejection { code: "json_wrong", detail: Some(path.to_string()) }),
                    _ => {}
                }
            }
            for p in &f.afgeleid {
                if let Some(s) = json_path(&v, p).and_then(|x| x.as_str()) {
                    derived.insert(p.rsplit('.').next().unwrap_or(p).to_string(), s.to_string());
                }
            }
        }
    }
    Ok(derived)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids() -> Vec<&'static str> {
        all().iter().map(|m| m.id.as_str()).collect()
    }

    #[test]
    fn built_in_manifests_are_valid_and_cover_the_required_integrations() {
        for m in all() {
            check(m, &ids()).unwrap_or_else(|e| panic!("{e}"));
        }
        for id in ["apple", "google_play", "supabase", "firebase", "local_db", "github", "vercel", "purchases", "revenuecat"] {
            assert!(get(id).is_some(), "{id}");
        }
        assert!(get("local_db").unwrap().velden.is_empty(), "local SQLite needs no secrets");
        let envs: Vec<&str> = all().iter().flat_map(|m| m.velden.iter().filter_map(|f| f.env_var.as_deref())).collect();
        let mut sorted = envs.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(envs.len(), sorted.len(), "environment variable names are unique");
    }

    #[test]
    fn every_text_key_exists_in_both_languages() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/i18n");
        let load = |l: &str| -> serde_json::Value { serde_json::from_str(&std::fs::read_to_string(root.join(format!("{l}.json"))).unwrap()).unwrap() };
        let (en, nl) = (load("en"), load("nl"));
        let has = |v: &serde_json::Value, key: &str| json_path(v, key).and_then(|x| x.as_str()).is_some();
        for m in all() {
            let mut keys = vec![m.naam.clone(), m.beschrijving.clone()];
            for f in &m.velden {
                keys.push(f.label.clone());
                keys.push(f.hulptekst.clone());
            }
            for k in keys {
                assert!(has(&en, &k), "en: {k}");
                assert!(has(&nl, &k), "nl: {k}");
            }
        }
    }

    #[test]
    fn check_rejects_broken_manifests() {
        let mut m = get("apple").unwrap().clone();
        m.afhankelijkheden = vec!["nope".into()];
        assert!(check(&m, &ids()).unwrap_err().contains("unknown dependency"));
        let mut m = get("apple").unwrap().clone();
        m.velden[0].validatie.regex = Some("(".into());
        assert!(check(&m, &ids()).unwrap_err().contains("bad regex"));
        let mut m = get("supabase").unwrap().clone();
        m.velden.push(m.velden[0].clone());
        assert!(check(&m, &ids()).unwrap_err().contains("duplicate"));
        let mut m = get("supabase").unwrap().clone();
        m.velden[0].scope = Scope::Global;
        assert!(check(&m, &ids()).unwrap_err().contains("global field"));
        let mut m = get("github").unwrap().clone();
        m.checklist_items = vec!["bogus".into()];
        assert!(check(&m, &ids()).is_err());
        assert!(parse("{\"id\":1}").is_err());
    }

    fn field(m: &str, f: &str) -> &'static Field {
        get(m).unwrap().field(f).unwrap()
    }

    #[test]
    fn text_validation_follows_the_manifest() {
        assert!(validate_text(field("apple", "key_id"), "ABCDE12345").is_ok());
        assert_eq!(validate_text(field("apple", "key_id"), "abc").unwrap_err().code, "pattern");
        assert_eq!(validate_text(field("apple", "key_id"), "  ").unwrap_err().code, "empty");
        assert!(validate_text(field("apple", "issuer_id"), "69a6de7e-1234-47e3-e053-5b8c7c11a4d1").is_ok());
        assert_eq!(validate_text(field("google_play", "keystore_password"), "abc").unwrap_err(), Rejection { code: "too_short", detail: Some("6".into()) });
        assert_eq!(validate_text(field("supabase", "url"), "supabase.co").unwrap_err().code, "pattern");
        assert!(validate_text(field("github", "token"), "github_pat_11AAAAAAAAAAAAAAAAAAAA_bbbbbbbb").is_ok());
        assert_eq!(validate_text(field("github", "token"), "password123").unwrap_err().code, "pattern");
        assert_eq!(validate_text(field("supabase", "url"), "https://a.supabase.co\nhttps://b").unwrap_err().code, "multiline");
        assert_eq!(validate_text(field("apple", "private_key"), "x").unwrap_err().code, "not_text");
    }

    fn jwt(role: &str) -> String {
        use base64::Engine;
        let e = |s: &str| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(s);
        format!("{}.{}.sig", e("{\"alg\":\"HS256\"}"), e(&format!("{{\"role\":\"{role}\"}}")))
    }

    #[test]
    fn a_secret_supabase_key_is_refused_in_the_public_field() {
        let public = field("supabase", "publishable_key");
        assert!(validate_text(public, "sb_publishable_abcdefghijkl").is_ok());
        assert!(validate_text(public, &jwt("anon")).is_ok());
        assert_eq!(validate_text(public, &jwt("service_role")).unwrap_err().code, "secret_in_public_field");
        assert_eq!(validate_text(public, "sb_secret_abcdefghijkl").unwrap_err().code, "pattern");
        assert!(validate_text(field("supabase", "secret_key"), &jwt("service_role")).is_ok());
        assert_eq!(jwt_role("garbage"), None);
    }

    #[test]
    fn file_validation_checks_type_content_and_reads_derived_values() {
        let p8 = field("apple", "private_key");
        let pem = b"-----BEGIN PRIVATE KEY-----\nMIGTAgEA\n-----END PRIVATE KEY-----\n";
        assert!(validate_file(p8, "AuthKey_ABCDE12345.p8", pem).is_ok());
        assert_eq!(validate_file(p8, "key.txt", pem).unwrap_err().code, "file_type");
        assert_eq!(validate_file(p8, "key.p8", b"hello").unwrap_err().code, "content");
        assert_eq!(validate_file(p8, "key.p8", b"").unwrap_err().code, "empty");

        let sa = field("google_play", "service_account");
        let json = br#"{"type":"service_account","project_id":"my-proj","client_email":"bot@my-proj.iam.gserviceaccount.com","private_key":"-----BEGIN PRIVATE KEY-----\nabc\n-----END PRIVATE KEY-----\n"}"#;
        let d = validate_file(sa, "key.json", json).unwrap();
        assert_eq!(d["client_email"], "bot@my-proj.iam.gserviceaccount.com");
        assert_eq!(d["project_id"], "my-proj");
        assert!(!d.values().any(|v| v.contains("PRIVATE")), "no secret in the derived values");
        assert_eq!(validate_file(sa, "key.json", br#"{"type":"authorized_user"}"#).unwrap_err().code, "json_wrong");
        assert_eq!(validate_file(sa, "key.json", br#"{"type":"service_account","client_email":"x"}"#).unwrap_err().code, "json_missing");
        assert_eq!(validate_file(sa, "key.json", b"not json").unwrap_err().code, "json_invalid");
        let big = vec![b'a'; 600 * 1024];
        assert_eq!(validate_file(sa, "key.json", &big).unwrap_err().code, "too_large");
        // Firebase: de afgeleide waarde komt uit een genest pad.
        let gs = field("firebase", "google_services_json");
        assert_eq!(validate_file(gs, "google-services.json", br#"{"project_info":{"project_id":"fb-1"}}"#).unwrap()["project_id"], "fb-1");
        assert!(validate_file(field("google_play", "keystore"), "my.jks", &[0xFE, 0xED, 0xFE, 0xED, 1, 2]).is_ok(), "binary keystores are fine");
    }
}

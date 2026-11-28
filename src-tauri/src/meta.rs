//! Projectgegevens die niet in de Flutter-bestanden thuishoren maar wel bij de app: vermelding in
//! de winkel, juridische links, vragenlijsten en bevestigingen. Staan in `.studio/publish.json` in
//! het project (gewone, niet-geheime gegevens; geheimen staan in de keychain).

use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Listing {
    pub title: String,
    pub short_description: String,
    pub full_description: String,
    pub category: String,
    pub contact_email: String,
}

/// Antwoorden op de privacy-/leeftijdsvragen: `None` = nog niet beantwoord.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Questionnaire {
    pub has_accounts: Option<bool>,
    pub collects_personal_data: Option<bool>,
    pub shows_ads: Option<bool>,
    pub in_app_purchases: Option<bool>,
    pub user_generated_content: Option<bool>,
    pub uses_location: Option<bool>,
    /// De gebruiker bevestigt dat de vragen in de Play Console zijn ingevuld.
    pub play_console_done: bool,
}

impl Questionnaire {
    pub fn all_answered(&self) -> bool {
        [self.has_accounts, self.collects_personal_data, self.shows_ads, self.in_app_purchases, self.user_generated_content, self.uses_location]
            .iter()
            .all(Option::is_some)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Legal {
    pub privacy_url: String,
    pub terms_url: String,
    /// De gebruiker heeft gecontroleerd dat de pagina's online staan.
    pub hosted_checked: bool,
    /// De app bevat een werkende "account verwijderen"-functie (verplicht bij accounts).
    pub account_deletion: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct DataConfig {
    /// local | firebase | supabase | none
    pub provider: String,
    pub supabase_url: String,
    pub supabase_project_ref: String,
    pub auth_methods: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Publishing {
    /// internal | closed | open
    pub track: String,
    pub play_account_ready: bool,
    pub final_confirmed: bool,
    /// Gedane handmatige tests van native functies (sleutel → gedaan).
    pub manual_checks: BTreeMap<String, bool>,
    pub device_tested: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Meta {
    pub listing: Listing,
    pub questionnaire: Questionnaire,
    pub legal: Legal,
    pub data: DataConfig,
    pub publishing: Publishing,
    /// Commit waarop de laatste upload naar de winkel plaatsvond (voor "versie verhoogd?").
    pub last_published_commit: Option<String>,
    pub last_published_version: Option<String>,
    /// Buildnummer van die upload: Google Play en de App Store eisen dat het elke keer hoger wordt.
    pub last_published_build: Option<u32>,
}

fn path(dir: &Path) -> std::path::PathBuf {
    dir.join(".studio/publish.json")
}

pub fn load(dir: &Path) -> AppResult<Meta> {
    match std::fs::read_to_string(path(dir)) {
        Ok(t) => Ok(serde_json::from_str(&t)?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Meta::default()),
        Err(e) => Err(e.into()),
    }
}

pub fn save(dir: &Path, meta: &Meta) -> AppResult<()> {
    validate(meta)?;
    std::fs::create_dir_all(dir.join(".studio"))?;
    let tmp = path(dir).with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(meta)? + "\n")?;
    std::fs::rename(tmp, path(dir))?;
    Ok(())
}

fn validate(m: &Meta) -> AppResult<()> {
    let https = |u: &str| u.is_empty() || (u.starts_with("https://") && u.len() < 300 && !u.contains(char::is_whitespace));
    if !https(&m.legal.privacy_url) || !https(&m.legal.terms_url) {
        return Err(AppError::Invalid("links must start with https://".into()));
    }
    if m.listing.title.chars().count() > 30 {
        return Err(AppError::Invalid("store title can be at most 30 characters".into()));
    }
    if m.listing.short_description.chars().count() > 80 {
        return Err(AppError::Invalid("short description can be at most 80 characters".into()));
    }
    if m.listing.full_description.chars().count() > 4000 {
        return Err(AppError::Invalid("full description can be at most 4000 characters".into()));
    }
    if !m.listing.contact_email.is_empty() && !(m.listing.contact_email.contains('@') && m.listing.contact_email.contains('.')) {
        return Err(AppError::Invalid("contact email does not look right".into()));
    }
    if !m.data.provider.is_empty() && !["local", "firebase", "supabase", "none"].contains(&m.data.provider.as_str()) {
        return Err(AppError::Invalid("unknown data provider".into()));
    }
    if !["", "internal", "closed", "open"].contains(&m.publishing.track.as_str()) {
        return Err(AppError::Invalid("unknown release track".into()));
    }
    for a in &m.data.auth_methods {
        if !["email", "google", "apple", "anonymous"].contains(&a.as_str()) {
            return Err(AppError::Invalid("unknown login method".into()));
        }
    }
    let ok_ref = |r: &str| r.chars().all(|c| c.is_ascii_alphanumeric());
    if !ok_ref(&m.data.supabase_project_ref) {
        return Err(AppError::Invalid("invalid Supabase project reference".into()));
    }
    if !(m.data.supabase_url.is_empty() || m.data.supabase_url.starts_with("https://")) {
        return Err(AppError::Invalid("the Supabase URL must start with https://".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("studio-meta-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn roundtrip_and_defaults() {
        let d = tmp();
        assert_eq!(load(&d).unwrap(), Meta::default());
        let mut m = Meta::default();
        m.listing.title = "Friday Scores".into();
        m.questionnaire.has_accounts = Some(false);
        m.publishing.manual_checks.insert("camera".into(), true);
        save(&d, &m).unwrap();
        assert_eq!(load(&d).unwrap(), m);
        assert!(std::fs::read_to_string(d.join(".studio/publish.json")).unwrap().contains("\"shortDescription\""));
    }

    #[test]
    fn tolerates_missing_and_extra_fields() {
        let d = tmp();
        std::fs::create_dir_all(d.join(".studio")).unwrap();
        std::fs::write(d.join(".studio/publish.json"), r#"{"listing":{"title":"X"},"somethingNew":1}"#).unwrap();
        assert_eq!(load(&d).unwrap().listing.title, "X");
    }

    #[test]
    fn validates_limits_and_links() {
        let d = tmp();
        let mut m = Meta::default();
        m.legal.privacy_url = "http://insecure.example".into();
        assert!(save(&d, &m).is_err());
        m.legal.privacy_url = "https://example.com/privacy".into();
        m.listing.title = "x".repeat(31);
        assert!(save(&d, &m).is_err());
        m.listing.title = "ok".into();
        m.listing.contact_email = "nope".into();
        assert!(save(&d, &m).is_err());
        m.listing.contact_email = "me@example.com".into();
        m.data.supabase_project_ref = "abc; rm".into();
        assert!(save(&d, &m).is_err());
        m.data.supabase_project_ref = "abcdefghij".into();
        save(&d, &m).unwrap();
    }

    #[test]
    fn questionnaire_completeness() {
        let mut q = Questionnaire::default();
        assert!(!q.all_answered());
        q.has_accounts = Some(true);
        q.collects_personal_data = Some(true);
        q.shows_ads = Some(false);
        q.in_app_purchases = Some(false);
        q.user_generated_content = Some(false);
        assert!(!q.all_answered());
        q.uses_location = Some(false);
        assert!(q.all_answered());
    }
}

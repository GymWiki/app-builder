//! Geheimen (API-sleutels, wachtwoorden van keystores) staan alleen in de OS-keychain
//! (Windows Credential Manager, macOS Keychain, Linux keyutils). Waarden gaan nooit terug naar
//! de UI, nooit naar logs en nooit in projectbestanden: de frontend kan alleen zetten,
//! controleren of iets bestaat en verwijderen.

use crate::error::{AppError, AppResult};

const SERVICE: &str = "app.studio.desktop";

/// Vaste lijst namen; de UI kan geen willekeurige keychain-items aanraken.
pub const NAMES: &[&str] = &[
    "supabase_token",
    "android_keystore_password",
    "play_service_account_json",
];

fn entry(project_id: &str, name: &str) -> AppResult<keyring::Entry> {
    if !NAMES.contains(&name) {
        return Err(AppError::Invalid(format!("unknown secret {name}")));
    }
    if project_id.is_empty() || !project_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err(AppError::Invalid("invalid project id".into()));
    }
    keyring::Entry::new(SERVICE, &format!("{project_id}:{name}")).map_err(|e| AppError::Keychain(e.to_string()))
}

pub fn set(project_id: &str, name: &str, value: &str) -> AppResult<()> {
    entry(project_id, name)?.set_password(value).map_err(|e| AppError::Keychain(e.to_string()))
}

/// Alleen voor intern gebruik (bv. een build); nooit als command beschikbaar.
pub fn get(project_id: &str, name: &str) -> AppResult<Option<String>> {
    match entry(project_id, name)?.get_password() {
        Ok(v) => Ok(Some(v)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(AppError::Keychain(e.to_string())),
    }
}

pub fn has(project_id: &str, name: &str) -> AppResult<bool> {
    Ok(get(project_id, name)?.is_some())
}

pub fn delete(project_id: &str, name: &str) -> AppResult<()> {
    match entry(project_id, name)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(AppError::Keychain(e.to_string())),
    }
}

/// Willekeurig wachtwoord voor een nieuwe keystore (zonder tekens die shells of gradle storen).
pub fn random_password() -> String {
    format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple())[..40].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unknown_names_and_ids() {
        assert!(matches!(set("abc", "evil_name", "x"), Err(AppError::Invalid(_))));
        assert!(matches!(set("../x", "supabase_token", "x"), Err(AppError::Invalid(_))));
        assert!(matches!(set("", "supabase_token", "x"), Err(AppError::Invalid(_))));
    }

    #[test]
    fn passwords_are_long_and_safe() {
        let p = random_password();
        assert_eq!(p.len(), 40);
        assert!(p.chars().all(|c| c.is_ascii_alphanumeric()));
        assert_ne!(p, random_password());
    }

    /// Echte keychain nodig (op Windows/macOS altijd; op Linux met keyutils). Niet overal beschikbaar.
    #[test]
    #[ignore]
    fn real_keychain_roundtrip() {
        let id = format!("test-{}", uuid::Uuid::new_v4());
        assert!(!has(&id, "supabase_token").unwrap());
        set(&id, "supabase_token", "s3cret").unwrap();
        assert_eq!(get(&id, "supabase_token").unwrap().as_deref(), Some("s3cret"));
        delete(&id, "supabase_token").unwrap();
        assert!(!has(&id, "supabase_token").unwrap());
    }
}

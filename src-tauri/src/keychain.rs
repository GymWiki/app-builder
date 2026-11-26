//! Verbinding tussen de oude, vaste geheimnamen en de kluis (`vault.rs`). Waarden gaan nooit terug naar de UI, nooit naar
//! logs en nooit in projectbestanden. Nieuwe code gebruikt de kluis rechtstreeks; deze functies bestaan zodat de
//! signing- en publicatiecode en de Data-pagina dezelfde opslag delen als de Geheimen-pagina.

use crate::error::{AppError, AppResult};
use crate::vault;

/// (integratie, veld, is een bestand)
fn map(name: &str) -> AppResult<(&'static str, &'static str, bool)> {
    match name {
        "supabase_token" => Ok(("supabase", "access_token", false)),
        "android_keystore_password" => Ok(("google_play", "keystore_password", false)),
        "play_service_account_json" => Ok(("google_play", "service_account", true)),
        other => Err(AppError::Invalid(format!("unknown secret {other}"))),
    }
}

pub fn set(project_id: &str, name: &str, value: &str) -> AppResult<()> {
    let (i, f, file) = map(name)?;
    let v = vault::global();
    if file {
        v.set_file(project_id, i, f, "service-account.json", value.as_bytes()).map(|_| ())
    } else {
        v.set_text(project_id, i, f, value)
    }
}

/// Alleen voor intern gebruik (bv. een build); nooit als command beschikbaar.
pub fn get(project_id: &str, name: &str) -> AppResult<Option<String>> {
    let (i, f, _) = map(name)?;
    vault::global().get_text(project_id, i, f)
}

pub fn has(project_id: &str, name: &str) -> AppResult<bool> {
    let (i, f, _) = map(name)?;
    Ok(vault::global().has(project_id, i, f))
}

pub fn delete(project_id: &str, name: &str) -> AppResult<()> {
    let (i, f, _) = map(name)?;
    vault::global().delete(project_id, i, f)
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
        assert!(matches!(set("../x", "supabase_token", "sbp_aaaaaaaaaaaaaaaaaaaaaaaa"), Err(AppError::Invalid(_))));
        assert!(matches!(set("", "supabase_token", "sbp_aaaaaaaaaaaaaaaaaaaaaaaa"), Err(AppError::Invalid(_))));
    }

    #[test]
    fn passwords_are_long_and_safe() {
        let p = random_password();
        assert_eq!(p.len(), 40);
        assert!(p.chars().all(|c| c.is_ascii_alphanumeric()));
        assert_ne!(p, random_password());
    }
}

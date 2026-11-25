//! Android-signing: een upload-keystore maken met `keytool` en de wachtwoorden in de keychain
//! bewaren. Wachtwoorden gaan nooit via de opdrachtregel (`-storepass:env`) en nooit in een
//! projectbestand: de Gradle-configuratie leest `STUDIO_KEYSTORE_*` uit de omgeving, die Studio
//! alleen tijdens een build vanuit de keychain meegeeft.

use crate::error::{AppError, AppResult};
use crate::{keychain, proc};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const ALIAS: &str = "upload";

/// `Java binary at: C:\...\jbr\bin\java.exe` uit `flutter doctor -v`.
pub fn java_from_doctor(doctor: &str) -> Option<PathBuf> {
    doctor.lines().find_map(|l| l.split_once("Java binary at:").map(|(_, p)| PathBuf::from(p.trim())))
}

pub fn find_keytool(flutter: Option<&Path>) -> Option<PathBuf> {
    let exe = if cfg!(windows) { "keytool.exe" } else { "keytool" };
    // 1. de Java die Flutter zelf gebruikt (Android Studio's JDK)
    if let Some(f) = flutter {
        if let Ok(o) = proc::run(f, &["doctor", "-v"], None, Duration::from_secs(180)) {
            if let Some(java) = java_from_doctor(&o.combined()) {
                let k = java.with_file_name(exe);
                if k.is_file() {
                    return Some(k);
                }
            }
        }
    }
    // 2. JAVA_HOME, 3. PATH, 4. bekende Android Studio-locaties
    if let Ok(h) = std::env::var("JAVA_HOME") {
        let k = PathBuf::from(h).join("bin").join(exe);
        if k.is_file() {
            return Some(k);
        }
    }
    if let Some(k) = proc::find("keytool") {
        return Some(k);
    }
    let guesses = ["C:\\Program Files\\Android\\Android Studio\\jbr\\bin\\keytool.exe", "/Applications/Android Studio.app/Contents/jbr/Contents/Home/bin/keytool"];
    guesses.iter().map(PathBuf::from).find(|p| p.is_file())
}

pub fn keystore_path(data_dir: &Path, project_id: &str) -> PathBuf {
    data_dir.join("keystores").join(project_id).join("upload-keystore.jks")
}

fn clean(s: &str, fallback: &str) -> String {
    let t: String = s.chars().filter(|c| !matches!(c, ',' | '"' | '\\' | '=' | '+' | '<' | '>' | '#' | ';' | '\n' | '\r')).collect();
    let t = t.trim();
    if t.is_empty() { fallback.to_string() } else { t.chars().take(60).collect() }
}

pub fn dname(name: &str, org: &str, country: &str) -> String {
    let c: String = country.chars().filter(|c| c.is_ascii_alphabetic()).take(2).collect::<String>().to_uppercase();
    format!("CN={}, O={}, C={}", clean(name, "App owner"), clean(org, "Studio"), if c.len() == 2 { c } else { "NL".into() })
}

/// Maakt de keystore en bewaart het wachtwoord in de keychain. Bestaat hij al, dan gebeurt er niets
/// (een keystore overschrijven zou het updaten van de gepubliceerde app onmogelijk maken).
pub fn create_keystore(keytool: &Path, data_dir: &Path, project_id: &str, dname: &str) -> AppResult<PathBuf> {
    let path = keystore_path(data_dir, project_id);
    if path.exists() {
        return Err(AppError::PathExists(path.display().to_string()));
    }
    std::fs::create_dir_all(path.parent().unwrap())?;
    let password = keychain::random_password();
    // Eerst in de keychain: lukt dat niet, dan maken we de keystore niet aan.
    keychain::set(project_id, "android_keystore_password", &password)?;
    let mut cmd = proc::command(keytool);
    cmd.env("STUDIO_KS_PW", &password).args([
        "-genkeypair",
        "-keystore",
        &path.to_string_lossy(),
        "-storetype",
        "PKCS12",
        "-keyalg",
        "RSA",
        "-keysize",
        "2048",
        "-validity",
        "10000",
        "-alias",
        ALIAS,
        "-storepass:env",
        "STUDIO_KS_PW",
        "-keypass:env",
        "STUDIO_KS_PW",
        "-dname",
        dname,
    ]);
    let out = cmd.output().map_err(|e| AppError::CommandFailed(format!("keytool: {e}")))?;
    if !out.status.success() {
        let _ = keychain::delete(project_id, "android_keystore_password");
        let _ = std::fs::remove_file(&path);
        return Err(AppError::CommandFailed(format!("keytool: {}", crate::logging::redact(String::from_utf8_lossy(&out.stderr).trim()))));
    }
    Ok(path)
}

pub fn gradle_configured(project_dir: &Path) -> bool {
    ["android/app/build.gradle.kts", "android/app/build.gradle"]
        .iter()
        .filter_map(|f| std::fs::read_to_string(project_dir.join(f)).ok())
        .any(|t| t.contains("STUDIO_KEYSTORE_PATH") && t.contains("signingConfig"))
}

/// Omgevingsvariabelen voor een release-build. Het wachtwoord komt uit de keychain.
pub fn build_env(project_id: &str, data_dir: &Path) -> AppResult<Vec<(String, String)>> {
    let path = keystore_path(data_dir, project_id);
    if !path.is_file() {
        return Err(AppError::NotFound("keystore".into()));
    }
    let pw = keychain::get(project_id, "android_keystore_password")?.ok_or_else(|| AppError::NotFound("keystore password in the keychain".into()))?;
    Ok(vec![
        ("STUDIO_KEYSTORE_PATH".into(), path.to_string_lossy().into_owned()),
        ("STUDIO_KEYSTORE_PASSWORD".into(), pw.clone()),
        ("STUDIO_KEY_ALIAS".into(), ALIAS.into()),
        ("STUDIO_KEY_PASSWORD".into(), pw),
    ])
}

/// Opdracht aan Claude om Gradle zo in te stellen dat de keystore uit de omgeving komt (geen wachtwoorden in bestanden).
pub const GRADLE_PROMPT: &str = "Configure release signing for the Android app. In android/app/build.gradle(.kts) add a `release` signingConfig that reads \
its values ONLY from these environment variables: STUDIO_KEYSTORE_PATH (storeFile), STUDIO_KEYSTORE_PASSWORD (storePassword), \
STUDIO_KEY_ALIAS (keyAlias) and STUDIO_KEY_PASSWORD (keyPassword), and use that signingConfig for the `release` build type. \
Never write passwords or a key.properties file. If the variables are missing the build must fail with a clear message instead of \
falling back to debug signing. Afterwards run `flutter analyze`.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_java_in_doctor_output() {
        let doc = "[✓] Android Studio (version 2024.1)\n    • Java binary at: C:\\Program Files\\Android\\Android Studio\\jbr\\bin\\java.exe\n    • Java version OpenJDK";
        assert_eq!(java_from_doctor(doc), Some(PathBuf::from("C:\\Program Files\\Android\\Android Studio\\jbr\\bin\\java.exe")));
        assert!(java_from_doctor("nothing").is_none());
    }

    #[test]
    fn distinguished_names_are_sanitised() {
        assert_eq!(dname("Jan, \"de\" Bakker", "Bakker+Co;", "nl"), "CN=Jan de Bakker, O=BakkerCo, C=NL");
        assert_eq!(dname("", "", "x1"), "CN=App owner, O=Studio, C=NL");
    }

    #[test]
    fn detects_gradle_signing_via_environment() {
        let d = std::env::temp_dir().join(format!("studio-gradle-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(d.join("android/app")).unwrap();
        assert!(!gradle_configured(&d));
        std::fs::write(d.join("android/app/build.gradle.kts"), "signingConfigs { create(\"release\") { storeFile = file(System.getenv(\"STUDIO_KEYSTORE_PATH\")) } }\nbuildTypes { release { signingConfig = signingConfigs.getByName(\"release\") } }").unwrap();
        assert!(gradle_configured(&d));
    }

    #[test]
    fn real_keytool_creates_a_keystore_and_never_overwrites() {
        let Some(keytool) = proc::find("keytool") else { return };
        let data = std::env::temp_dir().join(format!("studio-ks-{}", uuid::Uuid::new_v4()));
        let id = format!("t{}", uuid::Uuid::new_v4().simple());
        if keychain::set(&id, "android_keystore_password", "probe").is_err() {
            return; // geen keychain beschikbaar op deze machine
        }
        let _ = keychain::delete(&id, "android_keystore_password");
        let path = create_keystore(&keytool, &data, &id, &dname("Test User", "Test", "NL")).unwrap();
        assert!(path.is_file() && std::fs::metadata(&path).unwrap().len() > 500);
        let env = build_env(&id, &data).unwrap();
        let pw = env.iter().find(|(k, _)| k == "STUDIO_KEYSTORE_PASSWORD").unwrap().1.clone();
        assert_eq!(pw.len(), 40);
        // het wachtwoord staat in niets wat op schijf in de projectmap komt; keytool bevestigt dat het klopt
        let o = proc::run(&keytool, &["-list", "-keystore", &path.to_string_lossy(), "-storepass", &pw], None, Duration::from_secs(20)).unwrap();
        assert!(o.ok() && o.stdout.contains("upload"));
        assert!(matches!(create_keystore(&keytool, &data, &id, "CN=x"), Err(AppError::PathExists(_))));
        keychain::delete(&id, "android_keystore_password").unwrap();
        assert!(build_env(&id, &data).is_err());
    }
}

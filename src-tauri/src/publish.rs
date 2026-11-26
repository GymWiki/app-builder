//! App Bundle bouwen en uploaden. Android eerst (Google Play via Fastlane `supply`); iOS heeft
//! een eigen implementatie nodig (cloud-build) en meldt dat nu eerlijk. De upload gaat naar een
//! testtrack als **concept** (draft); naar productie publiceren doet de gebruiker zelf in de Play Console.

use crate::error::{AppError, AppResult};
use crate::proc;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const AAB_REL: &str = "build/app/outputs/bundle/release/app-release.aab";

/// Gemeenschappelijke interface voor winkels.
pub trait StorePublisher {
    fn id(&self) -> &'static str;
    fn build(&self, flutter: &Path, dir: &Path, env: &[(String, String)], on_line: &mut dyn FnMut(&str)) -> AppResult<PathBuf>;
    fn upload(&self, req: &UploadRequest, on_line: &mut dyn FnMut(&str)) -> AppResult<()>;
}

pub struct UploadRequest<'a> {
    pub project_id: &'a str,
    pub package: &'a str,
    pub artifact: &'a Path,
    pub track: &'a str,
    pub fastlane: &'a Path,
}

pub struct PlayPublisher;
pub struct AppStorePublisher;

pub fn fastlane_args(package: &str, aab: &Path, track: &str, json_key: &Path) -> Vec<String> {
    vec![
        "supply".into(),
        "--aab".into(),
        aab.to_string_lossy().into_owned(),
        "--package_name".into(),
        package.into(),
        "--track".into(),
        track.into(),
        "--json_key".into(),
        json_key.to_string_lossy().into_owned(),
        // Een nieuwe app mag via de API alleen concept-releases krijgen; de gebruiker rondt af in de Play Console.
        "--release_status".into(),
        "draft".into(),
        "--skip_upload_metadata".into(),
        "--skip_upload_images".into(),
        "--skip_upload_screenshots".into(),
    ]
}

pub fn play_track(track: &str) -> AppResult<&'static str> {
    match track {
        "internal" => Ok("internal"),
        "closed" => Ok("alpha"),
        other => Err(AppError::Invalid(format!("track {other} is not supported: Studio only uploads to internal or closed testing"))),
    }
}

/// Schrijft `{"REVENUECAT_SDK_KEY": "<waarde>"}` naar een tijdelijk bestand (alleen leesbaar voor de gebruiker).
fn write_define_file(value: &str) -> AppResult<PathBuf> {
    let path = std::env::temp_dir().join(format!("studio-defines-{}.json", uuid::Uuid::new_v4()));
    std::fs::write(&path, serde_json::json!({ "REVENUECAT_SDK_KEY": value }).to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(path)
}

impl StorePublisher for PlayPublisher {
    fn id(&self) -> &'static str {
        "google-play"
    }

    fn build(&self, flutter: &Path, dir: &Path, env: &[(String, String)], on_line: &mut dyn FnMut(&str)) -> AppResult<PathBuf> {
        // De publieke RevenueCat-sleutel gaat via een tijdelijk bestand mee (`--dart-define-from-file`), niet als argument van het
        // proces en nooit in de projectbestanden; het bestand verdwijnt direct na de build.
        let key_file = env.iter().find(|(k, _)| k == "STUDIO_REVENUECAT_SDK_KEY").map(|(_, v)| write_define_file(v)).transpose()?;
        let mut args: Vec<String> = vec!["build".into(), "appbundle".into(), "--release".into()];
        if let Some(f) = &key_file {
            args.push(format!("--dart-define-from-file={}", f.display()));
        }
        let args_ref: Vec<&str> = args.iter().map(String::as_str).collect();
        let code = proc::run_streaming(flutter, &args_ref, Some(dir), env, Duration::from_secs(1800), on_line);
        if let Some(f) = &key_file {
            let _ = std::fs::remove_file(f);
        }
        let code = code?;
        let aab = dir.join(AAB_REL);
        if code != Some(0) || !aab.is_file() {
            return Err(AppError::CommandFailed("flutter build appbundle failed; see the build log".into()));
        }
        Ok(aab)
    }

    fn upload(&self, req: &UploadRequest, on_line: &mut dyn FnMut(&str)) -> AppResult<()> {
        let track = play_track(req.track)?;
        // De sleutel staat alleen tijdens de upload als ontsleuteld tijdelijk bestand op schijf (alleen eigenaar); het
        // bestand verdwijnt zodra `key` wordt weggegooid.
        let key = crate::vault::global().materialize(req.project_id, "google_play", "service_account", "upload")?.ok_or_else(|| AppError::NotFound("Google Play service account key".into()))?;
        let args = fastlane_args(req.package, req.artifact, track, key.path());
        let argv: Vec<&str> = args.iter().map(String::as_str).collect();
        let res = proc::run_streaming(req.fastlane, &argv, None, &[], Duration::from_secs(900), on_line);
        drop(key);
        match res? {
            Some(0) => Ok(()),
            _ => Err(AppError::CommandFailed("fastlane supply failed; see the upload log".into())),
        }
    }
}

impl StorePublisher for AppStorePublisher {
    fn id(&self) -> &'static str {
        "app-store"
    }
    fn build(&self, _: &Path, _: &Path, _: &[(String, String)], _: &mut dyn FnMut(&str)) -> AppResult<PathBuf> {
        Err(AppError::Unsupported("ios_publish".into()))
    }
    fn upload(&self, _: &UploadRequest, _: &mut dyn FnMut(&str)) -> AppResult<()> {
        Err(AppError::Unsupported("ios_publish".into()))
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn script(path: &Path, body: &str) {
        std::fs::write(path, format!("#!/bin/sh\n{body}")).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[test]
    fn the_revenuecat_key_reaches_the_build_through_a_temporary_file_that_is_removed_afterwards() {
        let d = std::env::temp_dir().join(format!("studio-build-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        let fake = d.join("flutter");
        // De nep-flutter leest het bestand dat `--dart-define-from-file` noemt, en onthoudt waar het stond.
        script(&fake, "echo \"args: $@\"\nf=$(echo \"$@\" | sed -n 's/.*--dart-define-from-file=\\([^ ]*\\).*/\\1/p')\necho \"$f\" > defines-path.txt\ncat \"$f\" > defines-content.txt\nmkdir -p build/app/outputs/bundle/release\necho aab > build/app/outputs/bundle/release/app-release.aab\n");
        let secret = "appl_ABCDEFGHIJKLMNOP1234";
        let env = vec![("STUDIO_REVENUECAT_SDK_KEY".to_string(), secret.to_string())];
        let mut lines = vec![];
        PlayPublisher.build(&fake, &d, &env, &mut |l| lines.push(l.to_string())).unwrap();
        let args = lines.iter().find(|l| l.starts_with("args:")).unwrap();
        assert!(args.contains("--dart-define-from-file=") && !args.contains(secret), "the key is never an argument: {args}");
        assert!(std::fs::read_to_string(d.join("defines-content.txt")).unwrap().contains(secret), "the build could read it");
        let path = std::fs::read_to_string(d.join("defines-path.txt")).unwrap();
        assert!(!Path::new(path.trim()).exists(), "the temporary file is gone after the build");
        std::fs::remove_dir_all(d).ok();
    }

    #[test]
    fn maps_tracks_and_refuses_production() {
        assert_eq!(play_track("internal").unwrap(), "internal");
        assert_eq!(play_track("closed").unwrap(), "alpha");
        assert!(matches!(play_track("production"), Err(AppError::Invalid(_))));
        assert!(matches!(play_track("open"), Err(AppError::Invalid(_))));
    }

    #[test]
    fn builds_a_bundle_with_signing_env_and_redacts_the_log() {
        let d = std::env::temp_dir().join(format!("studio-build-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        let fake = d.join("flutter");
        script(&fake, "echo \"args: $@\"\necho \"path=$STUDIO_KEYSTORE_PATH pw=$STUDIO_KEYSTORE_PASSWORD token=$STUDIO_KEY_PASSWORD\"\nmkdir -p build/app/outputs/bundle/release\necho aab > build/app/outputs/bundle/release/app-release.aab\n");
        let env = vec![("STUDIO_KEYSTORE_PATH".to_string(), "/k.jks".to_string()), ("STUDIO_KEYSTORE_PASSWORD".to_string(), "hunter2hunter2".to_string()), ("STUDIO_KEY_PASSWORD".to_string(), "hunter2hunter2".to_string())];
        let mut lines = vec![];
        let aab = PlayPublisher.build(&fake, &d, &env, &mut |l| lines.push(l.to_string())).unwrap();
        assert!(aab.ends_with("app-release.aab"));
        assert_eq!(lines[0], "args: build appbundle --release");
        assert!(lines[1].contains("path=/k.jks"), "the env reached the build");
        assert!(!lines.join("\n").contains("hunter2hunter2"), "passwords must be redacted in the log: {lines:?}");
        script(&fake, "echo fail; exit 1\n");
        assert!(PlayPublisher.build(&fake, &d, &env, &mut |_| {}).is_err());
    }

    #[test]
    fn fastlane_args_upload_as_draft_without_touching_the_listing() {
        let a = fastlane_args("nl.bakker.scores", Path::new("/b.aab"), "internal", Path::new("/k.json"));
        let j = a.join(" ");
        assert!(j.contains("--release_status draft") && j.contains("--track internal") && j.contains("--package_name nl.bakker.scores"));
        assert!(j.contains("--skip_upload_metadata") && j.contains("--skip_upload_screenshots"));
    }

    #[test]
    fn app_store_is_not_supported_yet() {
        let r = AppStorePublisher.build(Path::new("x"), Path::new("."), &[], &mut |_| {});
        assert!(matches!(r, Err(AppError::Unsupported(_))));
    }

    #[test]
    fn upload_writes_the_key_only_during_the_run() {
        let id = format!("t{}", uuid::Uuid::new_v4().simple());
        if crate::keychain::set(&id, "play_service_account_json", r#"{"type":"service_account","client_email":"bot@p.iam.gserviceaccount.com","private_key":"-----BEGIN PRIVATE KEY-----\nPUBLISHTESTKEY0123456789\n-----END PRIVATE KEY-----\n"}"#).is_err() {
            return;
        }
        let d = std::env::temp_dir().join(format!("studio-up-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        let fake = d.join("fastlane");
        // het script bewaart een kopie van de argumenten en meldt of het sleutelbestand bestond
        script(&fake, "echo \"$@\" > \"$(dirname \"$0\")/args.txt\"\nkey=$(echo \"$@\" | sed 's/.*--json_key \\([^ ]*\\).*/\\1/')\n[ -f \"$key\" ] && echo key-existed > \"$(dirname \"$0\")/seen.txt\"\necho uploading\n");
        let aab = d.join("a.aab");
        std::fs::write(&aab, "x").unwrap();
        let mut lines = vec![];
        let req = UploadRequest { project_id: &id, package: "nl.x.y", artifact: &aab, track: "internal", fastlane: &fake };
        PlayPublisher.upload(&req, &mut |l| lines.push(l.to_string())).unwrap();
        assert_eq!(std::fs::read_to_string(d.join("seen.txt")).unwrap().trim(), "key-existed");
        let args = std::fs::read_to_string(d.join("args.txt")).unwrap();
        let key_path = args.split("--json_key ").nth(1).unwrap().split(' ').next().unwrap().trim().to_string();
        assert!(!std::path::Path::new(&key_path).exists(), "the decrypted key file must be deleted afterwards");
        assert!(std::fs::read_to_string(d.join("args.txt")).unwrap().contains("--release_status draft"));
        // zonder sleutel in de keychain: nette fout
        crate::keychain::delete(&id, "play_service_account_json").unwrap();
        assert!(matches!(PlayPublisher.upload(&req, &mut |_| {}), Err(AppError::NotFound(_))));
    }
}

//! Instellingen die in de platformbestanden van een Flutter-project staan: appnaam, bundle-ID,
//! minimale OS-versie en permissies. Alles werkt met nauwkeurige tekstbewerking (geen volledige
//! XML/plist-parser) en meldt per bestand wat is aangepast en wat niet kon, want de sjablonen van
//! `flutter create` verschillen per versie.

use crate::error::{AppError, AppResult};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct Applied {
    pub applied: Vec<String>,
    /// Bestanden/instellingen die niet automatisch aangepast konden worden, met reden.
    pub skipped: Vec<Skipped>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Skipped {
    pub target: String,
    pub reason: String,
}

impl Applied {
    fn ok(&mut self, t: &str) {
        self.applied.push(t.into());
    }
    fn skip(&mut self, t: &str, r: &str) {
        self.skipped.push(Skipped { target: t.into(), reason: r.into() });
    }
}

const MANIFEST: &str = "android/app/src/main/AndroidManifest.xml";
const PLIST: &str = "ios/Runner/Info.plist";
const PBXPROJ: &str = "ios/Runner.xcodeproj/project.pbxproj";
const FRAMEWORK_PLIST: &str = "ios/Flutter/AppFrameworkInfo.plist";

fn gradle_file(dir: &Path) -> Option<PathBuf> {
    ["android/app/build.gradle.kts", "android/app/build.gradle"].iter().map(|f| dir.join(f)).find(|p| p.is_file())
}

fn read(dir: &Path, rel: &str) -> Option<String> {
    std::fs::read_to_string(dir.join(rel)).ok()
}

fn write(dir: &Path, rel: &str, text: &str) -> AppResult<()> {
    Ok(std::fs::write(dir.join(rel), text)?)
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn unesc(s: &str) -> String {
    s.replace("&quot;", "\"").replace("&gt;", ">").replace("&lt;", "<").replace("&amp;", "&")
}

// ───────────── plist ─────────────

pub fn plist_get(text: &str, key: &str) -> Option<String> {
    let k = format!("<key>{key}</key>");
    let after = &text[text.find(&k)? + k.len()..];
    let s = after.find("<string>")?;
    // Moet het eerstvolgende element zijn.
    if !after[..s].trim().is_empty() {
        return None;
    }
    let e = after.find("</string>")?;
    Some(unesc(&after[s + 8..e]))
}

pub fn plist_set(text: &str, key: &str, value: &str) -> Option<String> {
    let k = format!("<key>{key}</key>");
    if let Some(i) = text.find(&k) {
        let after = i + k.len();
        let s = text[after..].find("<string>")? + after;
        let e = text[s..].find("</string>")? + s;
        if !text[after..s].trim().is_empty() {
            return None;
        }
        return Some(format!("{}<string>{}{}", &text[..s], esc(value), &text[e..]));
    }
    let end = text.rfind("</dict>")?;
    Some(format!("{}\t{k}\n\t<string>{}</string>\n{}", &text[..end], esc(value), &text[end..]))
}

pub fn plist_remove(text: &str, key: &str) -> String {
    let k = format!("<key>{key}</key>");
    let Some(i) = text.find(&k) else { return text.to_string() };
    let line_start = text[..i].rfind('\n').map(|p| p + 1).unwrap_or(0);
    let Some(e) = text[i..].find("</string>") else { return text.to_string() };
    let mut end = i + e + 9;
    if text[end..].starts_with('\n') {
        end += 1;
    }
    format!("{}{}", &text[..line_start], &text[end..])
}

// ───────────── manifest ─────────────

fn perm_line(name: &str) -> String {
    format!("    <uses-permission android:name=\"android.permission.{name}\"/>\n")
}

pub fn manifest_has(text: &str, name: &str) -> bool {
    text.contains(&format!("android:name=\"android.permission.{name}\""))
}

pub fn manifest_set(text: &str, name: &str, on: bool) -> Option<String> {
    let has = manifest_has(text, name);
    if on && !has {
        let i = text.find("<application")?;
        let line_start = text[..i].rfind('\n').map(|p| p + 1).unwrap_or(i);
        return Some(format!("{}{}{}", &text[..line_start], perm_line(name), &text[line_start..]));
    }
    if !on && has {
        return Some(text.lines().filter(|l| !l.contains(&format!("android.permission.{name}\""))).collect::<Vec<_>>().join("\n") + if text.ends_with('\n') { "\n" } else { "" });
    }
    Some(text.to_string())
}

// ───────────── permissies ─────────────

pub struct PermissionDef {
    pub key: &'static str,
    pub android: &'static [&'static str],
    pub ios_key: Option<&'static str>,
}

pub const PERMISSIONS: &[PermissionDef] = &[
    PermissionDef { key: "internet", android: &["INTERNET"], ios_key: None },
    PermissionDef { key: "camera", android: &["CAMERA"], ios_key: Some("NSCameraUsageDescription") },
    PermissionDef { key: "location", android: &["ACCESS_FINE_LOCATION", "ACCESS_COARSE_LOCATION"], ios_key: Some("NSLocationWhenInUseUsageDescription") },
    PermissionDef { key: "microphone", android: &["RECORD_AUDIO"], ios_key: Some("NSMicrophoneUsageDescription") },
    PermissionDef { key: "photos", android: &["READ_MEDIA_IMAGES"], ios_key: Some("NSPhotoLibraryUsageDescription") },
    PermissionDef { key: "notifications", android: &["POST_NOTIFICATIONS"], ios_key: None },
];

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PermissionState {
    pub key: String,
    pub enabled: bool,
    pub ios_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PlatformSettings {
    pub app_name: Option<String>,
    pub bundle_id: Option<String>,
    pub min_android: Option<u32>,
    pub min_ios: Option<String>,
    pub android_application_id: Option<String>,
    pub permissions: Vec<PermissionState>,
    pub has_android: bool,
    pub has_ios: bool,
}

fn gradle_value(text: &str, needle: &str) -> Option<String> {
    let line = text.lines().find(|l| l.trim_start().starts_with(needle))?;
    let rest = line.trim_start()[needle.len()..].trim_start_matches([' ', '=']).trim();
    Some(rest.trim_matches(['"', '\'']).to_string())
}

fn pbx_values(text: &str, key: &str) -> Vec<String> {
    let needle = format!("{key} = ");
    text.lines()
        .filter_map(|l| l.trim().strip_prefix(&needle).map(|r| r.trim_end_matches(';').trim_matches('"').to_string()))
        .collect()
}

pub fn read_settings(dir: &Path) -> PlatformSettings {
    let manifest = read(dir, MANIFEST);
    let plist = read(dir, PLIST);
    let gradle = gradle_file(dir).and_then(|p| std::fs::read_to_string(p).ok());
    let pbx = read(dir, PBXPROJ);

    let app_name = plist
        .as_deref()
        .and_then(|p| plist_get(p, "CFBundleDisplayName"))
        .or_else(|| manifest.as_deref().and_then(|m| m.split("android:label=\"").nth(1)).and_then(|r| r.split('"').next()).map(unesc));
    let bundle_id = pbx
        .as_deref()
        .and_then(|p| pbx_values(p, "PRODUCT_BUNDLE_IDENTIFIER").into_iter().find(|v| !v.ends_with(".RunnerTests")))
        .or_else(|| gradle.as_deref().and_then(|g| gradle_value(g, "applicationId")));
    let min_android = gradle.as_deref().and_then(|g| gradle_value(g, "minSdk")).and_then(|v| v.parse().ok());
    let min_ios = pbx.as_deref().and_then(|p| pbx_values(p, "IPHONEOS_DEPLOYMENT_TARGET").into_iter().next());
    let permissions = PERMISSIONS
        .iter()
        .map(|p| PermissionState {
            key: p.key.into(),
            enabled: manifest.as_deref().is_some_and(|m| p.android.iter().all(|a| manifest_has(m, a)))
                || p.ios_key.is_some_and(|k| plist.as_deref().and_then(|t| plist_get(t, k)).is_some()),
            ios_reason: p.ios_key.and_then(|k| plist.as_deref().and_then(|t| plist_get(t, k))),
        })
        .collect();
    let android_application_id = gradle.as_deref().and_then(|g| gradle_value(g, "applicationId"));
    PlatformSettings { app_name, bundle_id, min_android, min_ios, android_application_id, permissions, has_android: manifest.is_some(), has_ios: plist.is_some() }
}

// ───────────── schrijven ─────────────

pub fn set_app_name(dir: &Path, name: &str) -> AppResult<Applied> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 30 || name.contains(['\n', '\r']) {
        return Err(AppError::Invalid("app name must be 1-30 characters".into()));
    }
    let mut r = Applied::default();
    match read(dir, MANIFEST) {
        Some(m) if m.contains("android:label=\"") => {
            let i = m.find("android:label=\"").unwrap() + 15;
            let e = m[i..].find('"').unwrap() + i;
            write(dir, MANIFEST, &format!("{}{}{}", &m[..i], esc(name), &m[e..]))?;
            r.ok("Android: AndroidManifest.xml (label)");
        }
        _ => r.skip("Android", "android:label not found in AndroidManifest.xml"),
    }
    match read(dir, PLIST) {
        Some(p) => {
            let step1 = plist_set(&p, "CFBundleDisplayName", name);
            let step2 = step1.as_deref().and_then(|t| plist_set(t, "CFBundleName", name));
            match step2 {
                Some(t) => {
                    write(dir, PLIST, &t)?;
                    r.ok("iOS: Info.plist (CFBundleDisplayName, CFBundleName)");
                }
                None => r.skip("iOS", "Info.plist has an unexpected layout"),
            }
        }
        None => r.skip("iOS", "ios/Runner/Info.plist not found"),
    }
    Ok(r)
}

pub fn set_bundle_id(dir: &Path, new_id: &str) -> AppResult<Applied> {
    if !crate::projects::valid_org(new_id) {
        return Err(AppError::Invalid("bundle ID must look like com.yourname.app (lowercase letters, digits, underscores)".into()));
    }
    let mut r = Applied::default();
    // Android: applicationId (de namespace mag afwijken en blijft ongemoeid, zodat de code niet verplaatst hoeft).
    match gradle_file(dir) {
        Some(p) => {
            let text = std::fs::read_to_string(&p)?;
            let mut changed = false;
            let out: Vec<String> = text
                .lines()
                .map(|l| {
                    if l.trim_start().starts_with("applicationId") {
                        if let (Some(a), Some(b)) = (l.find(['"', '\'']), l.rfind(['"', '\''])) {
                            if b > a {
                                changed = true;
                                return format!("{}{}{}", &l[..=a], new_id, &l[b..]);
                            }
                        }
                    }
                    l.to_string()
                })
                .collect();
            if changed {
                std::fs::write(&p, out.join("\n") + if text.ends_with('\n') { "\n" } else { "" })?;
                r.ok("Android: applicationId");
            } else {
                r.skip("Android", "applicationId not found in build.gradle");
            }
        }
        None => r.skip("Android", "android/app/build.gradle(.kts) not found"),
    }
    match read(dir, PBXPROJ) {
        Some(p) => match pbx_values(&p, "PRODUCT_BUNDLE_IDENTIFIER").into_iter().find(|v| !v.ends_with(".RunnerTests")) {
            Some(old) => {
                write(dir, PBXPROJ, &p.replace(&old, new_id))?;
                r.ok("iOS: PRODUCT_BUNDLE_IDENTIFIER");
            }
            None => r.skip("iOS", "PRODUCT_BUNDLE_IDENTIFIER not found"),
        },
        None => r.skip("iOS", "project.pbxproj not found"),
    }
    Ok(r)
}

pub fn set_min_os(dir: &Path, android: Option<u32>, ios: Option<&str>) -> AppResult<Applied> {
    let mut r = Applied::default();
    if let Some(v) = android {
        if !(16..=40).contains(&v) {
            return Err(AppError::Invalid("minimum Android API level must be between 16 and 40".into()));
        }
        match gradle_file(dir) {
            Some(p) => {
                let text = std::fs::read_to_string(&p)?;
                let mut changed = false;
                let out: Vec<String> = text
                    .lines()
                    .map(|l| {
                        let t = l.trim_start();
                        let indent = &l[..l.len() - t.len()];
                        if t.starts_with("minSdkVersion") || t.starts_with("minSdk ") || t.starts_with("minSdk=") {
                            changed = true;
                            let kw = if t.starts_with("minSdkVersion") { "minSdkVersion" } else { "minSdk" };
                            let kts = p.extension().is_some_and(|e| e == "kts");
                            return if kts { format!("{indent}{kw} = {v}") } else { format!("{indent}{kw} {v}") };
                        }
                        l.to_string()
                    })
                    .collect();
                if changed {
                    std::fs::write(&p, out.join("\n") + if text.ends_with('\n') { "\n" } else { "" })?;
                    r.ok("Android: minSdk");
                } else {
                    r.skip("Android", "minSdk not found in build.gradle");
                }
            }
            None => r.skip("Android", "android/app/build.gradle(.kts) not found"),
        }
    }
    if let Some(v) = ios {
        if !v.chars().all(|c| c.is_ascii_digit() || c == '.') || v.is_empty() {
            return Err(AppError::Invalid("iOS version must look like 13.0".into()));
        }
        let mut any = false;
        if let Some(p) = read(dir, PBXPROJ) {
            let out: Vec<String> = p
                .lines()
                .map(|l| if l.trim_start().starts_with("IPHONEOS_DEPLOYMENT_TARGET = ") { any = true; let indent = &l[..l.len() - l.trim_start().len()]; format!("{indent}IPHONEOS_DEPLOYMENT_TARGET = {v};") } else { l.to_string() })
                .collect();
            write(dir, PBXPROJ, &(out.join("\n") + "\n"))?;
        }
        if let Some(p) = read(dir, FRAMEWORK_PLIST) {
            if let Some(t) = plist_set(&p, "MinimumOSVersion", v) {
                write(dir, FRAMEWORK_PLIST, &t)?;
                any = true;
            }
        }
        if any { r.ok("iOS: deployment target") } else { r.skip("iOS", "no deployment target found") }
    }
    Ok(r)
}

pub fn set_permission(dir: &Path, key: &str, enabled: bool, ios_reason: &str) -> AppResult<Applied> {
    let def = PERMISSIONS.iter().find(|p| p.key == key).ok_or_else(|| AppError::Invalid(format!("unknown permission {key}")))?;
    let mut r = Applied::default();
    match read(dir, MANIFEST) {
        Some(mut m) => {
            let mut ok = true;
            for a in def.android {
                match manifest_set(&m, a, enabled) {
                    Some(n) => m = n,
                    None => ok = false,
                }
            }
            if ok {
                write(dir, MANIFEST, &m)?;
                r.ok("Android: AndroidManifest.xml");
            } else {
                r.skip("Android", "no <application> element found in AndroidManifest.xml");
            }
        }
        None => r.skip("Android", "AndroidManifest.xml not found"),
    }
    if let Some(k) = def.ios_key {
        match read(dir, PLIST) {
            Some(p) => {
                let reason = if ios_reason.trim().is_empty() { "This app needs access to provide its features." } else { ios_reason.trim() };
                let out = if enabled { plist_set(&p, k, reason) } else { Some(plist_remove(&p, k)) };
                match out {
                    Some(t) => {
                        write(dir, PLIST, &t)?;
                        r.ok("iOS: Info.plist");
                    }
                    None => r.skip("iOS", "Info.plist has an unexpected layout"),
                }
            }
            None => r.skip("iOS", "Info.plist not found"),
        }
    }
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST_TXT: &str = "<manifest xmlns:android=\"http://schemas.android.com/apk/res/android\">\n    <application\n        android:label=\"my_app\"\n        android:name=\"${applicationName}\"\n        android:icon=\"@mipmap/ic_launcher\">\n    </application>\n</manifest>\n";
    const PLIST_TXT: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<plist version=\"1.0\">\n<dict>\n\t<key>CFBundleDisplayName</key>\n\t<string>My App</string>\n\t<key>CFBundleName</key>\n\t<string>my_app</string>\n</dict>\n</plist>\n";
    const GRADLE_KTS: &str = "android {\n    namespace = \"com.example.my_app\"\n    defaultConfig {\n        applicationId = \"com.example.my_app\"\n        minSdk = flutter.minSdkVersion\n        targetSdk = flutter.targetSdkVersion\n    }\n}\n";
    const PBX: &str = "\t\t\t\tIPHONEOS_DEPLOYMENT_TARGET = 12.0;\n\t\t\t\tPRODUCT_BUNDLE_IDENTIFIER = com.example.myApp;\n\t\t\t\tPRODUCT_BUNDLE_IDENTIFIER = com.example.myApp.RunnerTests;\n\t\t\t\tIPHONEOS_DEPLOYMENT_TARGET = 12.0;\n";

    fn project() -> PathBuf {
        let d = std::env::temp_dir().join(format!("studio-platform-{}", uuid::Uuid::new_v4()));
        for (rel, txt) in [(MANIFEST, MANIFEST_TXT), (PLIST, PLIST_TXT), ("android/app/build.gradle.kts", GRADLE_KTS), (PBXPROJ, PBX), (FRAMEWORK_PLIST, "<dict>\n\t<key>MinimumOSVersion</key>\n\t<string>12.0</string>\n</dict>\n")] {
            let p = d.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, txt).unwrap();
        }
        d
    }

    #[test]
    fn reads_current_values() {
        let s = read_settings(&project());
        assert_eq!(s.app_name.as_deref(), Some("My App"));
        assert_eq!(s.bundle_id.as_deref(), Some("com.example.myApp"));
        assert_eq!(s.min_ios.as_deref(), Some("12.0"));
        assert_eq!(s.min_android, None, "flutter.minSdkVersion is not a number");
        assert_eq!(s.android_application_id.as_deref(), Some("com.example.my_app"));
        assert!(s.has_android && s.has_ios);
        assert!(s.permissions.iter().all(|p| !p.enabled));
    }

    #[test]
    fn renames_the_app_on_both_platforms_and_escapes() {
        let d = project();
        let r = set_app_name(&d, "Jan & Piet's <App>").unwrap();
        assert_eq!(r.applied.len(), 2);
        assert!(std::fs::read_to_string(d.join(MANIFEST)).unwrap().contains("android:label=\"Jan &amp; Piet's &lt;App&gt;\""));
        assert_eq!(read_settings(&d).app_name.as_deref(), Some("Jan & Piet's <App>"));
        assert!(set_app_name(&d, "  ").is_err());
        assert!(set_app_name(&d, &"x".repeat(31)).is_err());
    }

    #[test]
    fn changes_bundle_id_everywhere_including_test_target() {
        let d = project();
        let r = set_bundle_id(&d, "nl.bakker.scores").unwrap();
        assert_eq!(r.applied.len(), 2, "{r:?}");
        let pbx = std::fs::read_to_string(d.join(PBXPROJ)).unwrap();
        assert!(pbx.contains("= nl.bakker.scores;") && pbx.contains("= nl.bakker.scores.RunnerTests;") && !pbx.contains("com.example"));
        assert!(std::fs::read_to_string(d.join("android/app/build.gradle.kts")).unwrap().contains("applicationId = \"nl.bakker.scores\""));
        assert!(std::fs::read_to_string(d.join("android/app/build.gradle.kts")).unwrap().contains("namespace = \"com.example.my_app\""));
        assert!(set_bundle_id(&d, "Bad Id").is_err());
        assert!(set_bundle_id(&d, "single").is_err());
    }

    #[test]
    fn sets_min_os_for_kts_and_groovy() {
        let d = project();
        let r = set_min_os(&d, Some(24), Some("14.0")).unwrap();
        assert_eq!(r.applied.len(), 2);
        let g = std::fs::read_to_string(d.join("android/app/build.gradle.kts")).unwrap();
        assert!(g.contains("        minSdk = 24\n") && g.contains("targetSdk"));
        assert!(!std::fs::read_to_string(d.join(PBXPROJ)).unwrap().contains("12.0"));
        assert!(std::fs::read_to_string(d.join(FRAMEWORK_PLIST)).unwrap().contains("<string>14.0</string>"));
        let s = read_settings(&d);
        assert_eq!((s.min_android, s.min_ios.as_deref()), (Some(24), Some("14.0")));
        // Groovy-stijl
        std::fs::remove_file(d.join("android/app/build.gradle.kts")).unwrap();
        std::fs::write(d.join("android/app/build.gradle"), "defaultConfig {\n    minSdkVersion flutter.minSdkVersion\n}\n").unwrap();
        set_min_os(&d, Some(26), None).unwrap();
        assert!(std::fs::read_to_string(d.join("android/app/build.gradle")).unwrap().contains("    minSdkVersion 26\n"));
        assert!(set_min_os(&d, Some(5), None).is_err());
        assert!(set_min_os(&d, None, Some("x;y")).is_err());
    }

    #[test]
    fn permissions_toggle_idempotently_with_ios_reason() {
        let d = project();
        set_permission(&d, "camera", true, "We use the camera to scan codes").unwrap();
        set_permission(&d, "camera", true, "We use the camera to scan codes and photos").unwrap(); // tweede keer: reden bijgewerkt, geen dubbele regel
        let m = std::fs::read_to_string(d.join(MANIFEST)).unwrap();
        assert_eq!(m.matches("android.permission.CAMERA").count(), 1);
        assert!(m.find("CAMERA").unwrap() < m.find("<application").unwrap());
        let s = read_settings(&d);
        let cam = s.permissions.iter().find(|p| p.key == "camera").unwrap();
        assert!(cam.enabled);
        assert_eq!(cam.ios_reason.as_deref(), Some("We use the camera to scan codes and photos"));
        set_permission(&d, "location", true, "").unwrap();
        assert!(std::fs::read_to_string(d.join(MANIFEST)).unwrap().contains("ACCESS_COARSE_LOCATION"));
        set_permission(&d, "camera", false, "").unwrap();
        assert!(!std::fs::read_to_string(d.join(MANIFEST)).unwrap().contains("CAMERA"));
        assert!(!std::fs::read_to_string(d.join(PLIST)).unwrap().contains("NSCameraUsageDescription"));
        assert!(std::fs::read_to_string(d.join(PLIST)).unwrap().contains("CFBundleName"));
        assert!(set_permission(&d, "teleport", true, "").is_err());
    }

    #[test]
    fn missing_files_are_reported_not_fatal() {
        let d = std::env::temp_dir().join(format!("studio-platform-empty-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        let r = set_app_name(&d, "X").unwrap();
        assert!(r.applied.is_empty() && r.skipped.len() == 2);
        let s = read_settings(&d);
        assert!(s.app_name.is_none() && s.bundle_id.is_none() && !s.has_android && !s.has_ios);
        assert!(s.permissions.iter().all(|p| !p.enabled));
    }
}

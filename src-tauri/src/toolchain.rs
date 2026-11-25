//! Controle van de benodigde tools: Git, Flutter SDK, Android-toolchain en Claude CLI.
//! Alleen versies en status worden gelezen — nooit tokens of inloggegevens.

use crate::error::AppResult;
use crate::proc;
use serde::Serialize;
use std::time::Duration;

/// Minimumversies. Geverifieerd tegen Claude Code 2.1.331 (`claude --help`,
/// `claude auth status --json`); pas aan als de docs iets anders zeggen.
pub const MIN_GIT: &str = "2.20.0";
pub const MIN_FLUTTER: &str = "3.24.0";
pub const MIN_CLAUDE: &str = "2.1.0";
/// De Dart-MCP-server (`dart mcp-server`) en de officiële Flutter-plugin vragen minstens Dart 3.9.
pub const MIN_DART: &str = "3.9.0";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ToolStatus {
    pub id: String,
    /// ok | missing | outdated | warning
    pub state: String,
    pub version: Option<String>,
    pub min_version: Option<String>,
    pub path: Option<String>,
    /// Technische details (bv. regels uit `flutter doctor`), alleen voor "toon details".
    pub detail: Option<String>,
    /// Herkende oorzaak, voor een gerichte uitleg in de UI (i18n-sleutel).
    pub issue: Option<String>,
}

impl ToolStatus {
    fn new(id: &str, min: Option<&str>) -> Self {
        Self {
            id: id.into(),
            state: "missing".into(),
            version: None,
            min_version: min.map(Into::into),
            path: None,
            detail: None,
            issue: None,
        }
    }
}

// ───────────── versies ─────────────

/// Haalt het eerste `x.y(.z)`-patroon uit een tekst.
pub fn parse_version(text: &str) -> Option<(u32, u32, u32)> {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() && (i == 0 || !bytes[i - 1].is_ascii_alphanumeric()) {
            let mut j = i;
            while j < bytes.len() && (bytes[j].is_ascii_digit() || bytes[j] == b'.') {
                j += 1;
            }
            let parts: Vec<u32> = text[i..j]
                .split('.')
                .filter(|s| !s.is_empty())
                .map_while(|s| s.parse().ok())
                .collect();
            if parts.len() >= 2 {
                return Some((parts[0], parts[1], parts.get(2).copied().unwrap_or(0)));
            }
            i = j;
        } else {
            i += 1;
        }
    }
    None
}

pub fn is_at_least(found: &str, min: &str) -> bool {
    match (parse_version(found), parse_version(min)) {
        (Some(f), Some(m)) => f >= m,
        _ => false,
    }
}

fn fmt_version(v: (u32, u32, u32)) -> String {
    format!("{}.{}.{}", v.0, v.1, v.2)
}

fn judge(mut st: ToolStatus, raw_version_text: &str) -> ToolStatus {
    match parse_version(raw_version_text) {
        Some(v) => {
            let ver = fmt_version(v);
            let min = st.min_version.clone().unwrap_or_default();
            st.state = if min.is_empty() || is_at_least(&ver, &min) { "ok" } else { "outdated" }.into();
            st.version = Some(ver);
        }
        None => {
            st.state = "warning".into();
            st.detail = Some(raw_version_text.trim().chars().take(300).collect());
        }
    }
    st
}

// ───────────── git / claude ─────────────

pub fn check_git() -> ToolStatus {
    let st = ToolStatus::new("git", Some(MIN_GIT));
    let Some(path) = proc::find("git") else { return st };
    let mut st = ToolStatus { path: Some(path.display().to_string()), ..st };
    match proc::run(&path, &["--version"], None, Duration::from_secs(10)) {
        Ok(o) if o.ok() => judge(st, &o.stdout),
        Ok(o) => {
            st.state = "warning".into();
            st.detail = Some(o.combined());
            st
        }
        Err(e) => {
            st.state = "warning".into();
            st.detail = Some(e.to_string());
            st
        }
    }
}

pub fn check_claude() -> ToolStatus {
    let st = ToolStatus::new("claude", Some(MIN_CLAUDE));
    let Some(path) = proc::find("claude") else { return st };
    let mut st = ToolStatus { path: Some(path.display().to_string()), ..st };
    match proc::run(&path, &["--version"], None, Duration::from_secs(20)) {
        Ok(o) if o.ok() => judge(st, &o.stdout),
        Ok(o) => {
            st.state = "warning".into();
            st.detail = Some(o.combined());
            st
        }
        Err(e) => {
            st.state = "warning".into();
            st.detail = Some(e.to_string());
            st
        }
    }
}

/// Alleen `loggedIn` uit `claude auth status --json`; verder wordt niets gelezen of bewaard.
pub fn parse_login_status(json: &str) -> Option<bool> {
    let v: serde_json::Value = serde_json::from_str(json.trim()).ok()?;
    v.get("loggedIn")?.as_bool()
}

pub fn claude_logged_in() -> AppResult<bool> {
    let Some(path) = proc::find("claude") else {
        return Err(crate::error::AppError::ToolMissing("claude".into()));
    };
    let o = proc::run(&path, &["auth", "status", "--json"], None, Duration::from_secs(30))?;
    // De CLI geeft bij "niet ingelogd" mogelijk een niet-nul exitcode, maar wel JSON.
    Ok(parse_login_status(&o.stdout).unwrap_or(false))
}

// ───────────── flutter / android ─────────────

/// `flutter --version --machine` kan ruis voor de JSON zetten (bv. "Building flutter tool...").
pub fn parse_flutter_machine_version(out: &str) -> Option<String> {
    let start = out.find('{')?;
    let v: serde_json::Value = serde_json::from_str(out[start..].trim()).ok()?;
    v.get("frameworkVersion")?.as_str().map(Into::into)
}

/// `dartSdkVersion` uit dezelfde JSON (de Dart die bij deze Flutter hoort).
pub fn parse_dart_version(out: &str) -> Option<String> {
    let start = out.find('{')?;
    let v: serde_json::Value = serde_json::from_str(out[start..].trim()).ok()?;
    v.get("dartSdkVersion")?.as_str().and_then(parse_version).map(fmt_version)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorSection {
    /// ok | warning | error | unknown
    pub level: &'static str,
    pub title: String,
    pub lines: Vec<String>,
}

fn marker_level(m: &str) -> Option<&'static str> {
    match m {
        "✓" | "√" | "v" => Some("ok"),
        "!" => Some("warning"),
        "✗" | "✘" | "x" | "X" | "☠" => Some("error"),
        "-" => Some("unknown"),
        _ => None,
    }
}

/// Splitst de uitvoer van `flutter doctor` in secties (`[✓] Titel` + onderliggende regels).
pub fn parse_doctor(output: &str) -> Vec<DoctorSection> {
    let mut sections: Vec<DoctorSection> = Vec::new();
    for line in output.lines() {
        let trimmed = line.trim_end();
        let header = trimmed.strip_prefix('[').and_then(|r| {
            let (marker, rest) = r.split_once(']')?;
            Some((marker_level(marker.trim())?, rest.trim()))
        });
        if let Some((level, title)) = header {
            sections.push(DoctorSection { level, title: title.to_string(), lines: vec![] });
        } else if trimmed.trim().is_empty() || trimmed.starts_with("Doctor summary") {
            continue;
        } else if trimmed.starts_with("• No issues") || trimmed.starts_with("! Doctor found") {
            break;
        } else if let Some(s) = sections.last_mut() {
            let l = trimmed.trim().trim_start_matches(['•', '✗', '!', '✓', '-', '√']).trim();
            if !l.is_empty() {
                s.lines.push(l.to_string());
            }
        }
    }
    sections
}

pub fn android_issue(section: &DoctorSection) -> Option<&'static str> {
    let text = section.lines.join(" ").to_lowercase() + " " + &section.title.to_lowercase();
    if text.contains("unable to locate android sdk") || text.contains("android sdk is missing") || text.contains("no android sdk") {
        Some("no_sdk")
    } else if text.contains("license") {
        Some("licenses")
    } else if text.contains("cmdline-tools") {
        Some("cmdline_tools")
    } else if section.level != "ok" {
        Some("other")
    } else {
        None
    }
}

pub fn android_status_from_doctor(sections: &[DoctorSection]) -> ToolStatus {
    let mut st = ToolStatus::new("android", None);
    let Some(sec) = sections.iter().find(|s| s.title.to_lowercase().starts_with("android toolchain")) else {
        st.state = "missing".into();
        st.issue = Some("no_sdk".into());
        return st;
    };
    st.version = sec.title.split("Android SDK version ").nth(1).and_then(|v| v.split(')').next()).map(|v| v.trim().to_string());
    st.detail = Some(sec.lines.join("\n")).filter(|d| !d.is_empty());
    match sec.level {
        "ok" => st.state = "ok".into(),
        _ => {
            st.issue = android_issue(sec).map(Into::into);
            st.state = if st.issue.as_deref() == Some("no_sdk") { "missing" } else { "warning" }.into();
        }
    }
    st
}

#[derive(Debug, Clone, Serialize)]
pub struct FlutterCheck {
    pub flutter: ToolStatus,
    pub android: ToolStatus,
}

pub fn check_flutter() -> FlutterCheck {
    let mut flutter = ToolStatus::new("flutter", Some(MIN_FLUTTER));
    let mut android = ToolStatus::new("android", None);
    let Some(path) = proc::find("flutter") else {
        android.issue = Some("needs_flutter".into());
        return FlutterCheck { flutter, android };
    };
    flutter.path = Some(path.display().to_string());
    match proc::run(&path, &["--version", "--machine"], None, Duration::from_secs(120)) {
        Ok(o) if o.ok() => {
            let text = parse_flutter_machine_version(&o.stdout).unwrap_or_else(|| o.stdout.clone());
            flutter = judge(flutter, &text);
            // Dart te oud voor de Dart/Flutter-MCP en de officiële plugin: eerlijk melden met een oplossing.
            if let (true, Some(dart)) = (flutter.state == "ok", parse_dart_version(&o.stdout)) {
                if !is_at_least(&dart, MIN_DART) {
                    flutter.state = "outdated".into();
                    flutter.issue = Some("dart_old".into());
                    flutter.detail = Some(format!("Dart {dart}"));
                    flutter.min_version = Some(format!("{} (Dart {MIN_DART})", flutter.min_version.clone().unwrap_or_default()));
                }
            }
        }
        Ok(o) => {
            flutter.state = "warning".into();
            flutter.detail = Some(o.combined().chars().take(600).collect());
        }
        Err(e) => {
            flutter.state = "warning".into();
            flutter.detail = Some(e.to_string());
        }
    }
    if flutter.state == "missing" || flutter.state == "warning" && flutter.version.is_none() {
        android.issue = Some("needs_flutter".into());
        return FlutterCheck { flutter, android };
    }
    match proc::run(&path, &["doctor"], None, Duration::from_secs(180)) {
        Ok(o) => android = android_status_from_doctor(&parse_doctor(&o.combined())),
        Err(e) => {
            android.state = "warning".into();
            android.detail = Some(e.to_string());
        }
    }
    FlutterCheck { flutter, android }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_versions_from_real_world_strings() {
        assert_eq!(parse_version("git version 2.43.0.windows.1"), Some((2, 43, 0)));
        assert_eq!(parse_version("2.1.331 (Claude Code)"), Some((2, 1, 331)));
        assert_eq!(parse_version("git version 2.39.5 (Apple Git-154)"), Some((2, 39, 5)));
        assert_eq!(parse_version("3.27"), Some((3, 27, 0)));
        assert_eq!(parse_version("no version here"), None);
    }

    #[test]
    fn compares_versions() {
        assert!(is_at_least("2.1.331", MIN_CLAUDE));
        assert!(is_at_least("2.1.0", MIN_CLAUDE));
        assert!(!is_at_least("1.9.9", MIN_CLAUDE));
        assert!(is_at_least("3.100.0", "3.24.0")); // numeriek, niet lexicografisch
        assert!(!is_at_least("garbage", "1.0.0"));
    }

    #[test]
    fn judge_marks_outdated() {
        let st = judge(ToolStatus::new("claude", Some(MIN_CLAUDE)), "1.0.3 (Claude Code)");
        assert_eq!(st.state, "outdated");
        assert_eq!(st.version.as_deref(), Some("1.0.3"));
        let st = judge(ToolStatus::new("claude", Some(MIN_CLAUDE)), "2.1.331 (Claude Code)");
        assert_eq!(st.state, "ok");
    }

    #[test]
    fn parses_login_json_without_reading_anything_else() {
        let ok = r#"{"loggedIn": true, "authMethod": "oauth_token", "apiProvider": "firstParty"}"#;
        assert_eq!(parse_login_status(ok), Some(true));
        assert_eq!(parse_login_status(r#"{"loggedIn": false}"#), Some(false));
        assert_eq!(parse_login_status("not json"), None);
    }

    #[test]
    fn dart_version_comes_from_the_flutter_json() {
        let out = "{\n  \"frameworkVersion\": \"3.50.1\",\n  \"dartSdkVersion\": \"3.14.0 (build 3.14.0-1.0.dev)\"\n}";
        assert_eq!(parse_dart_version(out).as_deref(), Some("3.14.0"));
        assert!(is_at_least("3.14.0", MIN_DART) && !is_at_least("3.8.1", MIN_DART));
        assert!(parse_dart_version("{}").is_none());
    }

    #[test]
    fn parses_machine_version_with_noise() {
        let out = "Building flutter tool...\n{\n  \"frameworkVersion\": \"3.27.1\",\n  \"channel\": \"stable\"\n}\n";
        assert_eq!(parse_flutter_machine_version(out).as_deref(), Some("3.27.1"));
    }

    const DOCTOR_OK: &str = "Doctor summary (to see all details, run flutter doctor -v):
[✓] Flutter (Channel stable, 3.27.1, on Microsoft Windows [Version 10.0.22631], locale nl-NL)
[✓] Windows Version (Installed version of Windows is version 10 or higher)
[✓] Android toolchain - develop for Android devices (Android SDK version 35.0.0)
[✓] Chrome - develop for the web
[!] Visual Studio - develop Windows apps
    ✗ Visual Studio not installed
[✓] Connected device (3 available)

! Doctor found issues in 1 category.
";

    #[test]
    fn doctor_sections_and_android_ok() {
        let secs = parse_doctor(DOCTOR_OK);
        assert_eq!(secs.len(), 6);
        assert_eq!(secs[4].level, "warning");
        assert_eq!(secs[4].lines, vec!["Visual Studio not installed"]);
        let a = android_status_from_doctor(&secs);
        assert_eq!(a.state, "ok");
        assert_eq!(a.version.as_deref(), Some("35.0.0"));
    }

    #[test]
    fn android_missing_sdk() {
        let out = "[✗] Android toolchain - develop for Android devices
    ✗ Unable to locate Android SDK.
      Install Android Studio from: https://developer.android.com/studio/index.html
[✓] Chrome - develop for the web
";
        let a = android_status_from_doctor(&parse_doctor(out));
        assert_eq!(a.state, "missing");
        assert_eq!(a.issue.as_deref(), Some("no_sdk"));
    }

    #[test]
    fn android_licenses_and_cmdline_tools() {
        let lic = "[!] Android toolchain - develop for Android devices (Android SDK version 34.0.0)
    ✗ Android license status unknown.
      Run `flutter doctor --android-licenses` to accept the SDK licenses.
";
        let a = android_status_from_doctor(&parse_doctor(lic));
        assert_eq!((a.state.as_str(), a.issue.as_deref()), ("warning", Some("licenses")));

        let cmd = "[✗] Android toolchain - develop for Android devices (Android SDK version 34.0.0)
    ✗ cmdline-tools component is missing
";
        let a = android_status_from_doctor(&parse_doctor(cmd));
        assert_eq!(a.issue.as_deref(), Some("cmdline_tools"));
    }

    #[test]
    fn windows_check_markers_are_understood() {
        let secs = parse_doctor("[√] Flutter (Channel stable)\n[X] Android toolchain - develop for Android devices\n");
        assert_eq!(secs[0].level, "ok");
        assert_eq!(secs[1].level, "error");
    }

    #[test]
    fn no_android_section_means_missing() {
        let a = android_status_from_doctor(&parse_doctor("[✓] Flutter (Channel stable)\n"));
        assert_eq!(a.state, "missing");
    }
}

//! Begeleide installatie. De frontend mag nooit zelf een commando kiezen: alleen
//! deze vaste lijst (per component en besturingssysteem) kan in de terminal draaien,
//! en pas nadat de gebruiker dat in de UI heeft bevestigd.

use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InstallPlan {
    pub program: String,
    pub args: Vec<String>,
    /// Leesbare weergave voor de bevestigingsdialoog.
    pub display: String,
}

fn plan(program: &str, args: &[&str]) -> InstallPlan {
    InstallPlan {
        program: program.into(),
        args: args.iter().map(|s| s.to_string()).collect(),
        display: std::iter::once(program).chain(args.iter().copied()).collect::<Vec<_>>().join(" "),
    }
}

/// `os` is `std::env::consts::OS` ("windows", "macos", "linux").
/// Winget-ID's: te controleren op een echte Windows-machine (zie DECISIONS.md).
pub fn install_plan(component: &str, os: &str) -> Option<InstallPlan> {
    match (component, os) {
        ("git", "windows") => Some(plan("winget", &["install", "--id", "Git.Git", "-e", "--source", "winget"])),
        ("claude", "windows") => Some(plan("winget", &["install", "--id", "Anthropic.ClaudeCode", "-e"])),
        ("android_licenses", _) => Some(plan("flutter", &["doctor", "--android-licenses"])),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_whitelisted_plans() {
        assert_eq!(install_plan("git", "windows").unwrap().display, "winget install --id Git.Git -e --source winget");
        assert!(install_plan("git", "linux").is_none());
        assert!(install_plan("flutter", "windows").is_none());
        assert!(install_plan("rm -rf /", "windows").is_none());
        assert_eq!(install_plan("android_licenses", "macos").unwrap().program, "flutter");
    }
}

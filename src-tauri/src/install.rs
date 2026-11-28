//! Begeleide installatie. De frontend mag nooit zelf een commando kiezen: alleen
//! deze vaste lijst (per component en besturingssysteem) kan in de terminal draaien,
//! en pas nadat de gebruiker dat in de UI heeft bevestigd.

use serde::Serialize;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

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
        // Zonder terminal, met pipes: de voorwaarden van winget en het pakket worden geaccepteerd nadat de gebruiker dit commando in de UI bevestigde.
        ("git", "windows") => Some(plan("winget", &["install", "--id", "Git.Git", "-e", "--source", "winget", "--silent", "--accept-package-agreements", "--accept-source-agreements"])),
        ("claude", "windows") => Some(plan("winget", &["install", "--id", "Anthropic.ClaudeCode", "-e", "--silent", "--accept-package-agreements", "--accept-source-agreements"])),
        // De Android-licenties lopen niet meer via een plan/terminal maar via `licenses.rs` (met een eigen toestemming).
        _ => None,
    }
}

/// Voert een vast installatieplan uit als gewoon subproces (pipes, geen terminal) en geeft elke uitvoerregel door. Geeft de exitcode terug.
pub fn run_streaming(program: &std::path::Path, args: &[String], env: &std::collections::HashMap<String, String>, timeout: Duration, line: &dyn Fn(&str)) -> (Option<i32>, String) {
    let mut cmd = Command::new(program);
    cmd.args(args).env_clear().envs(env).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return (None, format!("could not start {}: {e}", program.display())),
    };
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    for s in [child.stdout.take().map(|s| Box::new(s) as Box<dyn std::io::Read + Send>), child.stderr.take().map(|s| Box::new(s) as Box<dyn std::io::Read + Send>)].into_iter().flatten() {
        let tx = tx.clone();
        std::thread::spawn(move || {
            // winget tekent voortgang met \r; ook daarop in regels knippen
            let mut r = BufReader::new(s);
            let mut buf = vec![];
            while r.read_until(b'\n', &mut buf).map(|n| n > 0).unwrap_or(false) {
                let text = String::from_utf8_lossy(&buf).into_owned();
                for part in text.split(['\r', '\n']).map(str::trim).filter(|p| !p.is_empty()) {
                    let _ = tx.send(part.to_string());
                }
                buf.clear();
            }
        });
    }
    drop(tx);
    let started = Instant::now();
    let mut log = String::new();
    loop {
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(l) => {
                line(&l);
                log.push_str(&l);
                log.push('\n');
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
        }
        if started.elapsed() > timeout {
            let _ = child.kill();
            break;
        }
    }
    let code = child.wait().ok().and_then(|s| s.code());
    (code, log.chars().rev().take(6000).collect::<Vec<_>>().into_iter().rev().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_whitelisted_plans() {
        assert!(install_plan("git", "windows").unwrap().display.starts_with("winget install --id Git.Git -e --source winget --silent"));
        assert!(install_plan("git", "linux").is_none());
        assert!(install_plan("flutter", "windows").is_none());
        assert!(install_plan("rm -rf /", "windows").is_none());
        assert!(install_plan("android_licenses", "windows").is_none(), "licenses never go through a terminal plan");
    }

    #[cfg(unix)]
    #[test]
    fn streaming_gives_lines_and_the_exit_code_without_a_terminal() {
        let seen = std::sync::Mutex::new(vec![]);
        let env: std::collections::HashMap<String, String> = [("PATH".to_string(), std::env::var("PATH").unwrap_or_default())].into_iter().collect();
        let (code, log) = run_streaming(std::path::Path::new("/bin/sh"), &["-c".into(), "echo one; printf 'two\\rthree\\n'; exit 3".into()], &env, Duration::from_secs(5), &|l| seen.lock().unwrap().push(l.to_string()));
        assert_eq!(code, Some(3));
        assert_eq!(*seen.lock().unwrap(), vec!["one", "two", "three"]);
        assert!(log.contains("three"));
    }
}

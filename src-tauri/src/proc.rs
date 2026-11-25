//! Subprocessen starten zonder shell, met timeout en zonder consolevenster op Windows.

use crate::error::{AppError, AppResult};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct Output {
    pub status: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl Output {
    pub fn ok(&self) -> bool {
        self.status == Some(0)
    }
    pub fn combined(&self) -> String {
        format!("{}{}", self.stdout, self.stderr)
    }
}

/// Zoekt een programma in PATH (inclusief `.cmd`/`.bat` op Windows via PATHEXT).
pub fn find(program: &str) -> Option<PathBuf> {
    which::which(program).ok()
}

pub fn command(program: &Path) -> Command {
    // `.cmd`/`.bat`-bestanden (bv. npm-installaties van claude, flutter.bat) kunnen
    // niet direct worden gestart; Rust regelt dat zelf sinds 1.77 veilig.
    let mut c = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    c.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    c
}

pub fn run(program: &Path, args: &[&str], cwd: Option<&Path>, timeout: Duration) -> AppResult<Output> {
    let mut cmd = command(program);
    cmd.args(args);
    if let Some(d) = cwd {
        cmd.current_dir(d);
    }
    let label = format!("{} {}", program.file_name().map(|s| s.to_string_lossy()).unwrap_or_default(), args.join(" "));
    let mut child = cmd
        .spawn()
        .map_err(|e| AppError::CommandFailed(format!("{label}: {e}")))?;

    let mut out = child.stdout.take();
    let mut err = child.stderr.take();
    let t_out = std::thread::spawn(move || {
        let mut s = Vec::new();
        if let Some(o) = out.as_mut() {
            let _ = o.read_to_end(&mut s);
        }
        s
    });
    let t_err = std::thread::spawn(move || {
        let mut s = Vec::new();
        if let Some(o) = err.as_mut() {
            let _ = o.read_to_end(&mut s);
        }
        s
    });

    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait()? {
            Some(s) => break Some(s),
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            None => std::thread::sleep(Duration::from_millis(25)),
        }
    };
    let stdout = String::from_utf8_lossy(&t_out.join().unwrap_or_default()).into_owned();
    let stderr = String::from_utf8_lossy(&t_err.join().unwrap_or_default()).into_owned();
    match status {
        Some(s) => Ok(Output { status: s.code(), stdout, stderr }),
        None => Err(AppError::Timeout(label)),
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn captures_output_and_status() {
        let o = run(Path::new("/bin/sh"), &["-c", "echo hi; echo err >&2; exit 3"], None, Duration::from_secs(5)).unwrap();
        assert_eq!(o.status, Some(3));
        assert_eq!(o.stdout.trim(), "hi");
        assert_eq!(o.stderr.trim(), "err");
    }

    #[test]
    fn times_out() {
        let r = run(Path::new("/bin/sh"), &["-c", "sleep 5"], None, Duration::from_millis(200));
        assert!(matches!(r, Err(AppError::Timeout(_))));
    }

    #[test]
    fn missing_binary_is_reported() {
        assert!(run(Path::new("/nonexistent/xyz"), &[], None, Duration::from_secs(1)).is_err());
    }

    /// Een subproces dat een geheim uit de kluis uitprint (ook over meerdere regels) levert nooit de waarde aan de UI of log.
    #[cfg(unix)]
    #[test]
    fn subprocess_output_is_scrubbed_of_vault_values_and_env_values() {
        let key = format!("SUBPROCKEYLINE{}", uuid::Uuid::new_v4().simple());
        let pem = format!("-----BEGIN PRIVATE KEY-----\n{key}\n-----END PRIVATE KEY-----\n");
        crate::scrub::register("proc-test/key", &crate::scrub::values_from_file(pem.as_bytes()));
        let mut lines: Vec<String> = vec![];
        let script = format!("echo 'building…'; printf '%s\n' '-----BEGIN PRIVATE KEY-----' '{key}' '-----END PRIVATE KEY-----'; echo \"env=$STUDIO_TEST_SECRET\"");
        let env = vec![("STUDIO_TEST_SECRET".to_string(), "envsecret-1234567890".to_string())];
        let sh = std::path::Path::new("/bin/sh");
        run_streaming(sh, &["-c", &script], None, &env, Duration::from_secs(10), &mut |l| lines.push(l.to_string())).unwrap();
        let all = lines.join("\n");
        assert!(!all.contains(&key) && !all.contains("envsecret-1234567890"), "{all}");
        assert!(all.contains("building"), "normal output is untouched");
        crate::scrub::unregister("proc-test/key");
    }

}

/// Draait een programma en geeft de uitvoer (stdout + stderr) regel voor regel door. `env` wordt
/// alleen aan dit proces meegegeven. Geeft de exitcode terug (None bij time-out of beëindigd).
pub fn run_streaming(
    program: &Path,
    args: &[&str],
    cwd: Option<&Path>,
    env: &[(String, String)],
    timeout: Duration,
    on_line: &mut dyn FnMut(&str),
) -> AppResult<Option<i32>> {
    let mut cmd = command(program);
    cmd.args(args);
    for (k, v) in env {
        cmd.env(k, v);
    }
    if let Some(d) = cwd {
        cmd.current_dir(d);
    }
    let mut child = cmd.spawn().map_err(|e| AppError::CommandFailed(format!("{}: {e}", program.display())))?;
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let mut handles = vec![];
    let streams: [Option<Box<dyn Read + Send>>; 2] = [
        child.stdout.take().map(|s| Box::new(s) as Box<dyn Read + Send>),
        child.stderr.take().map(|s| Box::new(s) as Box<dyn Read + Send>),
    ];
    for s in streams.into_iter().flatten() {
        let tx = tx.clone();
        handles.push(std::thread::spawn(move || {
            for l in BufReader::new(s).lines().map_while(Result::ok) {
                let _ = tx.send(crate::logging::redact(&l));
            }
        }));
    }
    drop(tx);
    // Waarden die we zelf als omgeving meegeven (wachtwoorden!) komen nooit letterlijk in een log.
    let secrets: Vec<&str> = env.iter().map(|(_, v)| v.as_str()).filter(|v| v.len() >= 8).collect();
    let deadline = Instant::now() + timeout;
    loop {
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(mut l) => {
                for s in &secrets {
                    l = l.replace(s, "[redacted]");
                }
                on_line(&l)
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            Err(_) => {}
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(AppError::Timeout(program.display().to_string()));
        }
    }
    for h in handles {
        let _ = h.join();
    }
    Ok(child.wait()?.code())
}

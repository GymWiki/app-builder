//! Subprocessen starten zonder shell, met timeout en zonder consolevenster op Windows.

use crate::error::{AppError, AppResult};
use std::io::Read;
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
}

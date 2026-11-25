//! Git-hulpfuncties voor momentopnamen en status (alleen via de `git`-CLI).

use crate::error::{AppError, AppResult};
use crate::proc;
use std::path::Path;
use std::time::Duration;

fn git(dir: &Path, args: &[&str]) -> AppResult<String> {
    let git = proc::find("git").ok_or_else(|| AppError::ToolMissing("git".into()))?;
    let o = proc::run(&git, args, Some(dir), Duration::from_secs(60))?;
    if !o.ok() {
        return Err(AppError::CommandFailed(format!("git {}: {}", args.join(" "), o.combined().trim())));
    }
    Ok(o.stdout)
}

pub fn head(dir: &Path) -> AppResult<String> {
    Ok(git(dir, &["rev-parse", "HEAD"])?.trim().to_string())
}

pub fn changed_files(dir: &Path) -> AppResult<usize> {
    Ok(git(dir, &["status", "--porcelain"])?.lines().filter(|l| !l.trim().is_empty()).count())
}

pub fn branch(dir: &Path) -> AppResult<String> {
    Ok(git(dir, &["rev-parse", "--abbrev-ref", "HEAD"])?.trim().to_string())
}

pub fn last_commit_subject(dir: &Path) -> AppResult<String> {
    Ok(git(dir, &["log", "-1", "--pretty=%s"])?.trim().to_string())
}

/// Legt de huidige staat vast. `None` als er niets is veranderd.
pub fn snapshot(dir: &Path, label: &str) -> AppResult<Option<String>> {
    if changed_files(dir)? == 0 {
        return Ok(None);
    }
    git(dir, &["add", "-A"])?;
    git(dir, &["commit", "-m", label])?;
    Ok(Some(head(dir)?))
}

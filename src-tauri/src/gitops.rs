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

/// Bestanden die sinds `base` veranderd, toegevoegd of verwijderd zijn (ook nog niet vastgelegde).
pub fn changed_since(dir: &Path, base: &str) -> AppResult<Vec<String>> {
    let mut out: Vec<String> = git(dir, &["diff", "--name-only", base])?.lines().map(str::to_string).collect();
    out.extend(git(dir, &["ls-files", "--others", "--exclude-standard"])?.lines().map(str::to_string));
    out.retain(|l| !l.trim().is_empty());
    out.sort();
    out.dedup();
    Ok(out)
}

/// Zet losse bestanden terug naar de staat van `base` (nieuwe bestanden worden verwijderd).
pub fn restore_files(dir: &Path, base: &str, files: &[String]) -> AppResult<()> {
    for f in files {
        if f.starts_with('/') || f.split('/').any(|c| c == "..") {
            continue;
        }
        if git(dir, &["cat-file", "-e", &format!("{base}:{f}")]).is_ok() {
            git(dir, &["checkout", base, "--", f])?;
        } else {
            let _ = std::fs::remove_file(dir.join(f));
        }
    }
    Ok(())
}

/// Bestanden in de git-index.
pub fn tracked_files(dir: &Path) -> AppResult<Vec<String>> {
    Ok(git(dir, &["ls-files"])?.lines().map(str::to_string).filter(|l| !l.is_empty()).collect())
}

/// Wat een momentopname zou vastleggen: bestanden in de index plus nieuwe, niet-genegeerde bestanden.
pub fn visible_files(dir: &Path) -> AppResult<Vec<String>> {
    let mut v: Vec<String> = git(dir, &["ls-files", "--cached", "--others", "--exclude-standard"])?.lines().map(str::to_string).filter(|l| !l.is_empty()).collect();
    v.sort();
    v.dedup();
    Ok(v)
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

/// Herstelt de projectbestanden naar de staat van `commit`, als een **nieuwe** commit bovenop de
/// geschiedenis (er gaat niets verloren). Eerst wordt de huidige staat vastgelegd.
/// Geeft de nieuwe commit terug (of `None` als de inhoud al gelijk was).
pub fn restore(dir: &Path, commit: &str, label: &str) -> AppResult<Option<String>> {
    if commit.is_empty() || !commit.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(AppError::Invalid("invalid commit id".into()));
    }
    git(dir, &["cat-file", "-e", &format!("{commit}^{{commit}}")])?;
    // Alles wat er nu is eerst veiligstellen.
    let _ = snapshot(dir, "Before restoring an earlier version");
    // Bestanden die na `commit` zijn toegevoegd moeten weer weg.
    let added = git(dir, &["diff", "--name-only", "--diff-filter=A", commit, "HEAD"])?;
    git(dir, &["restore", &format!("--source={commit}"), "--staged", "--worktree", "--", "."])?;
    for f in added.lines().filter(|l| !l.trim().is_empty()) {
        let _ = git(dir, &["rm", "-q", "-f", "--", f]);
    }
    git(dir, &["add", "-A"])?;
    if git(dir, &["status", "--porcelain"])?.trim().is_empty() {
        return Ok(None);
    }
    git(dir, &["commit", "-m", label])?;
    Ok(Some(head(dir)?))
}

/// Alleen wat de app zelf is telt mee voor de testrun; `.studio/` (vermelding, links) en `store/` niet.
const CODE_PATHS: [&str; 7] = ["lib", "test", "assets", "pubspec.yaml", "android/app/src", "ios/Runner", "web"];
const SKIP_NAMES: [&str; 6] = ["GeneratedPluginRegistrant.java", "GeneratedPluginRegistrant.m", "GeneratedPluginRegistrant.h", "GeneratedPluginRegistrant.swift", ".DS_Store", "build"];

fn walk(path: &Path, rel: &str, out: &mut Vec<(String, std::path::PathBuf)>) {
    if path.is_file() {
        out.push((rel.to_string(), path.to_path_buf()));
    } else if let Ok(rd) = std::fs::read_dir(path) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if !SKIP_NAMES.contains(&name.as_str()) && !name.starts_with('.') {
                walk(&e.path(), &format!("{rel}/{name}"), out);
            }
        }
    }
}

/// Stabiele vingerafdruk van de huidige app-code: een hash over de inhoud van de codemappen in de
/// werkmap. Hij hangt dus niet af van git (committen verandert hem niet) en de AI-testrun is eraan
/// gekoppeld: elke codewijziging laat een eerdere run vervallen, een aangepaste winkelvermelding niet.
pub fn fingerprint(dir: &Path) -> AppResult<String> {
    let mut files = vec![];
    for p in CODE_PATHS {
        walk(&dir.join(p), p, &mut files);
    }
    files.sort();
    // FNV-1a: stabiel tussen versies van Studio (anders dan std's Hasher).
    let mut h: u64 = 0xcbf29ce484222325;
    let mut feed = |bytes: &[u8]| {
        for b in bytes {
            h ^= *b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
    };
    for (rel, path) in files {
        feed(rel.as_bytes());
        feed(&[0]);
        feed(&std::fs::read(path)?);
        feed(&[1]);
    }
    Ok(format!("{h:016x}"))
}

/// Commit-berichten sinds `since` (of alles), voor een changelog-concept.
pub fn log_since(dir: &Path, since: Option<&str>) -> AppResult<Vec<String>> {
    let range = since.map(|s| format!("{s}..HEAD"));
    let mut args = vec!["log", "--pretty=%s"];
    if let Some(r) = &range {
        args.push(r);
    }
    Ok(git(dir, &args)?.lines().filter(|l| !l.trim().is_empty()).map(str::to_string).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("studio-git-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        for a in [vec!["init", "-q"], vec!["config", "user.email", "t@t"], vec!["config", "user.name", "t"]] {
            git(&d, &a).unwrap();
        }
        d
    }

    #[test]
    fn restore_brings_back_old_state_without_losing_history() {
        if proc::find("git").is_none() {
            return;
        }
        let d = repo();
        std::fs::write(d.join("a.txt"), "one").unwrap();
        std::fs::write(d.join("keep.txt"), "keep").unwrap();
        let v1 = snapshot(&d, "v1").unwrap().unwrap();
        std::fs::write(d.join("a.txt"), "two").unwrap();
        std::fs::write(d.join("new.txt"), "added later").unwrap();
        std::fs::remove_file(d.join("keep.txt")).unwrap();
        snapshot(&d, "v2").unwrap();
        std::fs::write(d.join("a.txt"), "three-uncommitted").unwrap();

        let new = restore(&d, &v1, "Restored: v1").unwrap();
        assert!(new.is_some());
        assert_eq!(std::fs::read_to_string(d.join("a.txt")).unwrap(), "one");
        assert_eq!(std::fs::read_to_string(d.join("keep.txt")).unwrap(), "keep");
        assert!(!d.join("new.txt").exists());
        assert_eq!(changed_files(&d).unwrap(), 0);
        let log = log_since(&d, None).unwrap();
        // niets weggegooid: ook de ongecommitte versie is eerst vastgelegd
        assert!(log.contains(&"v2".to_string()) && log.contains(&"Before restoring an earlier version".to_string()));
        assert_eq!(log[0], "Restored: v1");
        // opnieuw herstellen naar dezelfde staat: niets te doen
        assert!(restore(&d, &v1, "again").unwrap().is_none());
        assert!(restore(&d, "zz;rm", "x").is_err());
    }

    #[test]
    fn fingerprint_changes_with_any_edit() {
        if proc::find("git").is_none() {
            return;
        }
        let d = repo();
        std::fs::create_dir_all(d.join("lib")).unwrap();
        std::fs::create_dir_all(d.join(".studio")).unwrap();
        std::fs::write(d.join("lib/a.dart"), "one").unwrap();
        snapshot(&d, "v1").unwrap();
        let clean = fingerprint(&d).unwrap();
        assert_eq!(clean, fingerprint(&d).unwrap(), "stable");
        std::fs::write(d.join("lib/a.dart"), "two").unwrap();
        let dirty = fingerprint(&d).unwrap();
        assert_ne!(clean, dirty);
        std::fs::write(d.join("lib/a.dart"), "three").unwrap();
        assert_ne!(dirty, fingerprint(&d).unwrap());
        std::fs::write(d.join("lib/a.dart"), "two").unwrap();
        assert_eq!(dirty, fingerprint(&d).unwrap(), "same edit = same fingerprint");
        // committen verandert de vingerafdruk niet; niet-code (.studio) ook niet
        snapshot(&d, "v2").unwrap();
        assert_eq!(dirty, fingerprint(&d).unwrap());
        std::fs::write(d.join(".studio/publish.json"), "{}").unwrap();
        assert_eq!(dirty, fingerprint(&d).unwrap(), "store listing edits must not expire the test run");
        snapshot(&d, "v3").unwrap();
        assert_eq!(dirty, fingerprint(&d).unwrap());
    }
}

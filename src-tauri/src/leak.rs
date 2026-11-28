//! Lekcontrole (fase 10d): zoekt in de projectbestanden naar geheimen. Draait vóór elke automatische momentopname, in de
//! checklist en vóór publicatie. Gezocht wordt naar (1) de exacte opgeslagen waarden uit de kluis, (2) gangbare
//! sleutelpatronen, (3) geheime bestanden die in git terecht zouden komen en (4) een service_role/secret-sleutel van
//! Supabase, die nooit in een app hoort. Een treffer bevat bestand, regel en soort, nooit de waarde zelf.

use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Leak {
    pub file: String,
    pub line: u32,
    /// known_secret | private_key | token | service_role_key | secret_file | service_account_file | env_file | secret_in_app_assets
    pub kind: String,
    /// Staat het bestand al in git? Dan moet de sleutel ook vervangen worden.
    pub tracked: bool,
    /// Kan Studio het bestand zelf naar de kluis verplaatsen?
    pub movable: bool,
}

const SKIP_DIRS: &[&str] = &[".git", "build", ".dart_tool", "node_modules", ".gradle", "Pods", ".idea", ".symlinks"];
const MAX_FILES: usize = 4000;
const MAX_BYTES: u64 = 1_000_000;

fn patterns() -> &'static [(&'static str, regex_lite::Regex)] {
    use std::sync::OnceLock;
    static P: OnceLock<Vec<(&'static str, regex_lite::Regex)>> = OnceLock::new();
    P.get_or_init(|| {
        let mk = |k, r: &str| (k, regex_lite::Regex::new(r).expect("valid leak pattern"));
        vec![
            mk("private_key", r"-----BEGIN (?:[A-Z]+ )?PRIVATE KEY-----"),
            mk("token", r"\b(?:gh[pousr]_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{30,})"),
            mk("token", r"\bAKIA[0-9A-Z]{16}\b"),
            mk("token", r"\bsk-ant-[A-Za-z0-9_-]{20,}"),
            mk("token", r"\bsk_live_[A-Za-z0-9]{16,}"),
            mk("token", r"\bsbp_[A-Za-z0-9]{30,}"),
            mk("service_role_key", r"\bsb_secret_[A-Za-z0-9_-]{8,}"),
        ]
    })
}

fn jwt_re() -> &'static regex_lite::Regex {
    use std::sync::OnceLock;
    static R: OnceLock<regex_lite::Regex> = OnceLock::new();
    R.get_or_init(|| regex_lite::Regex::new(r"eyJ[A-Za-z0-9_-]{8,}\.eyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}").expect("valid jwt pattern"))
}

fn file_kind(rel: &str) -> Option<(&'static str, bool)> {
    let name = rel.rsplit('/').next().unwrap_or(rel).to_lowercase();
    let ext = name.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    if name == ".env" || (name.starts_with(".env.") && !name.ends_with(".example") && !name.ends_with(".sample")) {
        return Some(("env_file", false));
    }
    match ext {
        "p8" | "jks" | "keystore" | "p12" | "pfx" => return Some(("secret_file", true)),
        _ => {}
    }
    if name == "key.properties" {
        return Some(("secret_file", false));
    }
    if (name.contains("service-account") || name.contains("service_account")) && ext == "json" {
        return Some(("service_account_file", true));
    }
    None
}

fn visible_files(dir: &Path) -> (Vec<String>, std::collections::HashSet<String>) {
    // Wat git zou vastleggen: bestanden in de index plus nieuwe, niet-genegeerde bestanden.
    let tracked: std::collections::HashSet<String> = crate::gitops::tracked_files(dir).unwrap_or_default().into_iter().collect();
    if let Ok(list) = crate::gitops::visible_files(dir) {
        return (list, tracked);
    }
    fn walk(root: &Path, d: &Path, out: &mut Vec<String>, depth: usize) {
        if depth > 12 || out.len() > MAX_FILES {
            return;
        }
        let Ok(rd) = std::fs::read_dir(d) else { return };
        for e in rd.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            if p.is_dir() {
                if !SKIP_DIRS.contains(&name.as_str()) {
                    walk(root, &p, out, depth + 1);
                }
            } else if let Ok(r) = p.strip_prefix(root) {
                out.push(r.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    let mut v = vec![];
    walk(dir, dir, &mut v, 0);
    (v, tracked)
}

fn assets_leak(dir: &Path) -> bool {
    let Ok(t) = std::fs::read_to_string(dir.join("pubspec.yaml")) else { return false };
    t.lines().map(str::trim).filter(|l| l.starts_with("- ")).any(|l| file_kind(l.trim_start_matches("- ").trim().trim_matches(['"', '\''])).is_some())
}

/// `known` zijn de exacte waarden (met varianten) uit de kluis.
pub fn scan(dir: &Path, known: &[String]) -> Vec<Leak> {
    let (files, tracked) = visible_files(dir);
    let mut out = vec![];
    for rel in files.iter().take(MAX_FILES) {
        if rel.split('/').any(|c| SKIP_DIRS.contains(&c)) {
            continue;
        }
        let is_tracked = tracked.contains(rel);
        if let Some((kind, movable)) = file_kind(rel) {
            out.push(Leak { file: rel.clone(), line: 0, kind: kind.into(), tracked: is_tracked, movable });
            continue;
        }
        let path = dir.join(rel);
        let Ok(meta) = std::fs::metadata(&path) else { continue };
        if meta.len() > MAX_BYTES || !meta.is_file() {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        let mut hit_kinds: Vec<(u32, &str)> = vec![];
        for (i, line) in text.lines().enumerate() {
            let n = i as u32 + 1;
            if known.iter().any(|k| line.contains(k.as_str())) {
                hit_kinds.push((n, "known_secret"));
                continue;
            }
            if let Some((k, _)) = patterns().iter().find(|(_, r)| r.is_match(line)) {
                hit_kinds.push((n, k));
                continue;
            }
            if jwt_re().find_iter(line).any(|m| crate::manifest::jwt_role(m.as_str()).as_deref() == Some("service_role")) {
                hit_kinds.push((n, "service_role_key"));
            }
        }
        // Eén melding per soort per bestand: genoeg om te blokkeren en te verklaren, zonder ruis.
        let mut seen = std::collections::HashSet::new();
        for (n, k) in hit_kinds {
            if seen.insert(k) {
                out.push(Leak { file: rel.clone(), line: n, kind: k.into(), tracked: is_tracked, movable: false });
            }
        }
    }
    if assets_leak(dir) {
        out.push(Leak { file: "pubspec.yaml".into(), line: 0, kind: "secret_in_app_assets".into(), tracked: true, movable: false });
    }
    out
}

/// De regels die voorkomen dat geheimen in git belanden; Studio zet ze in een eigen blok in `.gitignore`.
pub const IGNORE_LINES: &[&str] = &[".env", ".env.*", "!.env.example", "*.p8", "*.jks", "*.keystore", "*.p12", "*.pfx", "key.properties", "*service-account*.json", "*service_account*.json"];
const START: &str = "# studio:secrets:start";
const END: &str = "# studio:secrets:end";

/// Zet het blok in `.gitignore` (maakt het bestand zo nodig); eigen regels van de gebruiker blijven staan. Idempotent.
pub fn ensure_gitignore(dir: &Path) -> std::io::Result<bool> {
    let path = dir.join(".gitignore");
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let block = format!("{START}\n# Secrets never go in git. Studio keeps them in its own vault.\n{}\n{END}", IGNORE_LINES.join("\n"));
    let new = match (existing.find(START), existing.find(END)) {
        (Some(a), Some(b)) if b > a => format!("{}{}{}", &existing[..a], block, &existing[b + END.len()..]),
        _ if existing.trim().is_empty() => format!("{block}\n"),
        _ => format!("{}\n\n{block}\n", existing.trim_end()),
    };
    if new == existing {
        return Ok(false);
    }
    std::fs::write(&path, new)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("studio-leak-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(d.join("lib")).unwrap();
        std::fs::write(d.join("pubspec.yaml"), "name: demo\nflutter:\n  assets:\n    - assets/logo.png\n").unwrap();
        std::fs::write(d.join("lib/main.dart"), "void main() {}\n").unwrap();
        d
    }
    fn w(d: &Path, rel: &str, text: &str) {
        let p = d.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
    fn kinds(l: &[Leak]) -> Vec<(String, String)> {
        let mut v: Vec<_> = l.iter().map(|x| (x.file.clone(), x.kind.clone())).collect();
        v.sort();
        v
    }

    #[test]
    fn a_clean_project_has_no_leaks() {
        let d = project();
        assert!(scan(&d, &[]).is_empty());
        // Publieke Firebase-configuratie en een voorbeeldbestand zijn geen lek.
        w(&d, "android/app/google-services.json", r#"{"client":[{"api_key":[{"current_key":"AIzaSyA-publicpublicpublicpublicpublic12"}]}]}"#);
        w(&d, ".env.example", "SUPABASE_URL=\n");
        assert!(scan(&d, &[]).is_empty(), "{:?}", scan(&d, &[]));
    }

    #[test]
    fn finds_exact_vault_values_and_common_key_patterns_without_exposing_them() {
        let d = project();
        w(&d, "lib/config.dart", "const api = 'my-vault-value-0987654321';\n");
        w(&d, "lib/gh.dart", "const t = 'ghp_abcdefghijklmnopqrstuvwxyz0123456789';\n");
        w(&d, "lib/key.dart", "const k = '''-----BEGIN PRIVATE KEY-----\nabc\n-----END PRIVATE KEY-----''';\n");
        w(&d, "lib/aws.dart", "const a = 'AKIAABCDEFGHIJKLMNOP';\n");
        w(&d, "lib/sb.dart", "const s = 'sb_secret_abcdefghijkl';\n");
        let l = scan(&d, &["my-vault-value-0987654321".to_string()]);
        assert_eq!(
            kinds(&l),
            vec![
                ("lib/aws.dart".into(), "token".into()),
                ("lib/config.dart".into(), "known_secret".into()),
                ("lib/gh.dart".into(), "token".into()),
                ("lib/key.dart".into(), "private_key".into()),
                ("lib/sb.dart".into(), "service_role_key".into()),
            ]
        );
        assert_eq!(l.iter().find(|x| x.file == "lib/config.dart").unwrap().line, 1);
        let json = serde_json::to_string(&l).unwrap();
        assert!(!json.contains("my-vault-value") && !json.contains("ghp_abc") && !json.contains("AKIAABC"), "a finding never contains the value");
    }

    #[test]
    fn a_supabase_service_role_jwt_is_found_but_the_anon_key_is_fine() {
        use base64::Engine;
        let e = |s: &str| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(s);
        let jwt = |role: &str| format!("{}.{}.signaturesignature", e("{\"alg\":\"HS256\",\"typ\":\"JWT\"}"), e(&format!("{{\"role\":\"{role}\",\"iss\":\"supabase\"}}")));
        let d = project();
        w(&d, "lib/anon.dart", &format!("const anon = '{}';\n", jwt("anon")));
        assert!(scan(&d, &[]).is_empty(), "anon keys belong in apps");
        w(&d, "lib/admin.dart", &format!("const admin = '{}';\n", jwt("service_role")));
        assert_eq!(kinds(&scan(&d, &[])), vec![("lib/admin.dart".into(), "service_role_key".into())]);
    }

    #[test]
    fn secret_files_are_found_by_name_and_flagged_movable_when_the_vault_can_take_them() {
        let d = project();
        w(&d, "AuthKey_ABCDE12345.p8", "x");
        w(&d, "android/upload.jks", "x");
        w(&d, "android/key.properties", "storePassword=abc\n");
        w(&d, "config/my-service-account.json", "{}");
        w(&d, ".env", "A=b\n");
        let l = scan(&d, &[]);
        let by = |f: &str| l.iter().find(|x| x.file == f).unwrap();
        assert!(by("AuthKey_ABCDE12345.p8").movable && by("android/upload.jks").movable && by("config/my-service-account.json").movable);
        assert!(!by("android/key.properties").movable && !by(".env").movable);
        assert_eq!(by(".env").kind, "env_file");
        assert_eq!(l.len(), 5);
    }

    #[test]
    fn ignored_files_are_not_leaks_but_tracked_ones_are_marked() {
        let Some(git) = crate::proc::find("git") else { return };
        let d = project();
        let g = |a: &[&str]| crate::proc::run(&git, a, Some(&d), std::time::Duration::from_secs(10)).unwrap();
        g(&["init", "-q"]);
        g(&["config", "user.email", "t@t"]);
        g(&["config", "user.name", "t"]);
        ensure_gitignore(&d).unwrap();
        w(&d, ".env", "A=1\n");
        w(&d, "AuthKey.p8", "x");
        assert!(scan(&d, &[]).is_empty(), "ignored by the Studio block: git would never commit these");
        // Een eerder al vastgelegd bestand blijft een lek en wordt als 'tracked' gemarkeerd.
        w(&d, "lib/old.dart", "const k = 'ghp_abcdefghijklmnopqrstuvwxyz0123456789';\n");
        g(&["add", "-A"]);
        g(&["commit", "-qm", "x"]);
        let l = scan(&d, &[]);
        assert_eq!(l.len(), 1);
        assert!(l[0].tracked);
    }

    #[test]
    fn secrets_listed_as_app_assets_would_ship_inside_the_app() {
        let d = project();
        w(&d, "pubspec.yaml", "name: demo\nflutter:\n  assets:\n    - assets/logo.png\n    - .env\n");
        assert_eq!(kinds(&scan(&d, &[]))[0].1, "secret_in_app_assets");
    }

    #[test]
    fn generated_and_build_folders_are_skipped() {
        let d = project();
        w(&d, "build/app/out.txt", "ghp_abcdefghijklmnopqrstuvwxyz0123456789");
        w(&d, ".dart_tool/x.json", "ghp_abcdefghijklmnopqrstuvwxyz0123456789");
        assert!(scan(&d, &[]).is_empty());
    }

    #[test]
    fn gitignore_block_is_added_once_keeps_user_lines_and_can_be_updated() {
        let d = project();
        assert!(ensure_gitignore(&d).unwrap());
        assert!(!ensure_gitignore(&d).unwrap(), "idempotent");
        std::fs::write(d.join(".gitignore"), format!("my-own-line\n\n{}", std::fs::read_to_string(d.join(".gitignore")).unwrap())).unwrap();
        ensure_gitignore(&d).unwrap();
        let t = std::fs::read_to_string(d.join(".gitignore")).unwrap();
        assert!(t.starts_with("my-own-line") && t.matches("studio:secrets:start").count() == 1);
        for l in [".env", "*.p8", "*.jks", "key.properties"] {
            assert!(t.lines().any(|x| x == l), "{l}");
        }
        std::fs::write(d.join(".gitignore"), "build/\n").unwrap();
        ensure_gitignore(&d).unwrap();
        assert!(std::fs::read_to_string(d.join(".gitignore")).unwrap().starts_with("build/\n\n# studio:secrets:start"));
    }
}

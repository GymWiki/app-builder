//! Projectprofiel: alles wat Claude Code in een project nodig heeft om snel, goedkoop en
//! Flutter-correct te werken, zonder dat de gebruiker er iets van hoeft te weten.
//! Idempotent en veilig: bestanden van de gebruiker worden nooit overschreven; Studio-bestanden
//! worden alleen bijgewerkt als ze nog precies zo zijn als Studio ze schreef (manifest met hashes).

use crate::error::{AppError, AppResult};
use crate::proc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

/// Versie van het profiel. Verhogen bij elke inhoudelijke wijziging; bestaande projecten worden gemigreerd.
pub const CURRENT: u32 = 2;
pub const PLUGIN_ID: &str = "dart-flutter@dart-flutter";
pub const PLUGIN_SOURCE: &str = "flutter/agent-plugins";
pub const MIN_DART: &str = "3.9.0";

const MANIFEST: &str = ".studio/profile.json";
const BLOCK_START: &str = "<!-- studio:start -->";
const BLOCK_END: &str = "<!-- studio:end -->";
const FORMAT_HOOK: &str = ".claude/hooks/studio_format.dart";

/// De tekst van het oude (fase 1–5) `CLAUDE.md`, zodat die veilig vervangen kan worden.
const LEGACY_CLAUDE_MD: &str = "# Instructions for Claude\n\n@STUDIO.md\n";

pub struct ProjectFacts<'a> {
    pub name: &'a str,
    /// "feature-first" (nieuw) of "flat" (oud project zonder lib/core)
    pub structure: &'a str,
}

#[derive(Debug, Default, Serialize, PartialEq)]
pub struct ProfileReport {
    pub version: u32,
    pub written: Vec<String>,
    pub updated: Vec<String>,
    pub unchanged: Vec<String>,
    /// (pad, reden): bestanden die we bewust niet hebben aangeraakt.
    pub skipped: Vec<(String, String)>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Manifest {
    version: u32,
    files: BTreeMap<String, String>,
}

/// FNV-1a: stabiel tussen versies (geen cryptografie nodig; het gaat om "is dit nog mijn bestand").
pub fn hash(text: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in text.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

fn load_manifest(dir: &Path) -> Manifest {
    std::fs::read_to_string(dir.join(MANIFEST)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn save_manifest(dir: &Path, m: &Manifest) -> AppResult<()> {
    std::fs::create_dir_all(dir.join(".studio"))?;
    std::fs::write(dir.join(MANIFEST), serde_json::to_string_pretty(m)? + "\n")?;
    Ok(())
}

// ───────────── inhoud ─────────────

pub fn claude_block(f: &ProjectFacts, scope_tool: bool) -> String {
    let scope = if scope_tool {
        "- If you need to change files outside the task's scope, call the studio tool `breid_bereik_uit` (reason + files) first; Studio approves it automatically.\n"
    } else {
        ""
    };
    let legacy = if f.structure == "flat" {
        "- This project still has the older flat structure. Do not reorganize it unasked; put every NEW screen in lib/features/<screen>/ and shared code in lib/core/.\n"
    } else {
        ""
    };
    include_str!("../templates/profile/claude_block.md").replace("__NAME__", &f.name.replace(['\n', '\r'], " ")).replace("__SCOPE_RULE__", &format!("{scope}{legacy}"))
}

struct Owned {
    path: String,
    contents: String,
}

fn owned_files() -> Vec<Owned> {
    let mut v = vec![
        Owned { path: ".claude/rules/flutter.md".into(), contents: include_str!("../templates/profile/rules/flutter.md").into() },
        Owned { path: FORMAT_HOOK.into(), contents: include_str!("../templates/profile/hooks/studio_format.dart").into() },
    ];
    macro_rules! skill {
        ($n:literal, $r:literal) => {
            v.push(Owned { path: concat!(".claude/skills/", $n, "/SKILL.md").into(), contents: include_str!(concat!("../templates/profile/skills/", $n, "/SKILL.md")).into() });
            v.push(Owned { path: concat!(".claude/skills/", $n, "/references/", $r).into(), contents: include_str!(concat!("../templates/profile/skills/", $n, "/references/", $r)).into() });
        };
    }
    skill!("studio-screen", "structure.md");
    skill!("studio-theme", "fonts.md");
    skill!("studio-ui-kit", "kits.md");
    skill!("studio-data-auth", "accounts.md");
    skill!("studio-release", "checklist.md");
    skill!("studio-store-listing", "legal.md");
    v
}

/// Wat we in `.claude/settings.json` willen hebben (wordt samengevoegd, nooit vervangen).
pub fn desired_settings() -> Value {
    json!({
        "permissions": { "deny": [
            "Read(build/**)", "Read(.dart_tool/**)", "Read(.git/**)",
            "Read(*.g.dart)", "Read(*.freezed.dart)",
            "Read(android/.gradle/**)", "Read(ios/Pods/**)"
        ]},
        "hooks": { "PostToolUse": [{
            "matcher": "Edit|MultiEdit|Write",
            "hooks": [{ "type": "command", "command": format!("dart \"${{CLAUDE_PROJECT_DIR}}/{FORMAT_HOOK}\""), "timeout": 20 }]
        }]}
    })
}

fn is_studio_hook_entry(entry: &Value) -> bool {
    entry["hooks"].as_array().is_some_and(|hs| hs.iter().any(|h| h["command"].as_str().is_some_and(|c| c.contains("studio_format.dart"))))
}

/// Voegt `ours` samen in `existing`: objecten recursief, lijsten als vereniging, bestaande waarden winnen.
pub fn merge_json(existing: &mut Value, ours: &Value) {
    match (existing, ours) {
        (Value::Object(e), Value::Object(o)) => {
            for (k, v) in o {
                match e.get_mut(k) {
                    Some(cur) => merge_json(cur, v),
                    None => {
                        e.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        (Value::Array(e), Value::Array(o)) => {
            for v in o {
                if !e.contains(v) {
                    e.push(v.clone());
                }
            }
        }
        _ => {} // scalar: wat er staat blijft staan
    }
}

pub fn merge_settings(existing: &str) -> AppResult<String> {
    let mut v: Value = if existing.trim().is_empty() { Value::Object(Map::new()) } else { serde_json::from_str(existing)? };
    if !v.is_object() {
        return Err(AppError::Invalid("settings.json is not an object".into()));
    }
    // Oude Studio-hooks eerst weghalen, zodat een gewijzigde hook niet dubbel komt te staan.
    if let Some(arr) = v.pointer_mut("/hooks/PostToolUse").and_then(Value::as_array_mut) {
        arr.retain(|e| !is_studio_hook_entry(e));
    }
    merge_json(&mut v, &desired_settings());
    Ok(serde_json::to_string_pretty(&v)? + "\n")
}

// ───────────── toepassen ─────────────

fn write_file(dir: &Path, rel: &str, text: &str) -> AppResult<()> {
    let p = dir.join(rel);
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d)?;
    }
    Ok(std::fs::write(p, text)?)
}

/// Studio-bestand: aanmaken, of bijwerken als de gebruiker het niet heeft aangepast.
fn apply_owned(dir: &Path, man: &mut Manifest, file: &Owned, rep: &mut ProfileReport) -> AppResult<()> {
    let target = dir.join(&file.path);
    let new_hash = hash(&file.contents);
    match std::fs::read_to_string(&target) {
        Err(_) => {
            write_file(dir, &file.path, &file.contents)?;
            man.files.insert(file.path.clone(), new_hash);
            rep.written.push(file.path.clone());
        }
        Ok(current) => {
            let cur_hash = hash(&current);
            if cur_hash == new_hash {
                man.files.insert(file.path.clone(), new_hash);
                rep.unchanged.push(file.path.clone());
            } else if man.files.get(&file.path) == Some(&cur_hash) {
                write_file(dir, &file.path, &file.contents)?;
                man.files.insert(file.path.clone(), new_hash);
                rep.updated.push(file.path.clone());
            } else {
                rep.skipped.push((file.path.clone(), "changed by the user; left as is".into()));
            }
        }
    }
    Ok(())
}

fn apply_claude_md(dir: &Path, block: &str, rep: &mut ProfileReport) -> AppResult<()> {
    let path = dir.join("CLAUDE.md");
    let managed = block.to_string();
    let existing = std::fs::read_to_string(&path).ok();
    let new_text = match existing.as_deref() {
        None => managed.clone(),
        Some(t) if t == LEGACY_CLAUDE_MD || t.trim().is_empty() => managed.clone(),
        Some(t) => match (t.find(BLOCK_START), t.find(BLOCK_END)) {
            (Some(a), Some(b)) if b > a => format!("{}{}{}", &t[..a], managed.trim_end_matches('\n'), &t[b + BLOCK_END.len()..]),
            // De gebruiker (of Claude) heeft eigen tekst: ons blok komt erbij, hun tekst blijft staan.
            _ => format!("{}\n\n{}", t.trim_end(), managed),
        },
    };
    match existing {
        Some(ref t) if *t == new_text => rep.unchanged.push("CLAUDE.md".into()),
        Some(_) => {
            std::fs::write(&path, new_text)?;
            rep.updated.push("CLAUDE.md".into());
        }
        None => {
            std::fs::write(&path, new_text)?;
            rep.written.push("CLAUDE.md".into());
        }
    }
    Ok(())
}

fn apply_settings(dir: &Path, rep: &mut ProfileReport) -> AppResult<()> {
    let path = dir.join(".claude/settings.json");
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    match merge_settings(&existing) {
        Ok(text) => {
            if text == existing {
                rep.unchanged.push(".claude/settings.json".into());
            } else {
                write_file(dir, ".claude/settings.json", &text)?;
                (if existing.is_empty() { &mut rep.written } else { &mut rep.updated }).push(".claude/settings.json".into());
            }
        }
        Err(e) => rep.skipped.push((".claude/settings.json".into(), format!("not valid JSON ({e}); left as is"))),
    }
    Ok(())
}

/// Lokale afspraken per feature: alleen aanmaken als ze ontbreken (Claude mag ze daarna aanpassen).
fn apply_feature_notes(dir: &Path, rep: &mut ProfileReport) -> AppResult<()> {
    let Ok(rd) = std::fs::read_dir(dir.join("lib/features")) else { return Ok(()) };
    for e in rd.flatten().filter(|e| e.path().is_dir()) {
        let name = e.file_name().to_string_lossy().into_owned();
        let rel = format!("lib/features/{name}/CLAUDE.md");
        if !dir.join(&rel).exists() {
            write_file(dir, &rel, &format!("# Feature: {name}\n- Screen in `{name}_screen.dart`; logic in a Riverpod Notifier in `{name}_controller.dart`; widgets in `widgets/`.\n- Use `AppSpacing`/`AppRadius` and the color scheme; no hard-coded colors.\n- Shared models and widgets go to lib/core.\n"))?;
            rep.written.push(rel);
        }
    }
    Ok(())
}

/// Alle bestanden van het profiel schrijven of bijwerken. Veilig om vaker aan te roepen.
pub fn apply(dir: &Path, facts: &ProjectFacts) -> AppResult<ProfileReport> {
    let mut rep = ProfileReport { version: CURRENT, ..Default::default() };
    let mut man = load_manifest(dir);
    for f in owned_files() {
        apply_owned(dir, &mut man, &f, &mut rep)?;
    }
    apply_claude_md(dir, &claude_block(facts, true), &mut rep)?;
    apply_settings(dir, &mut rep)?;
    apply_feature_notes(dir, &mut rep)?;
    man.version = CURRENT;
    save_manifest(dir, &man)?;
    Ok(rep)
}

pub fn structure_of(dir: &Path) -> &'static str {
    if dir.join("lib/core").is_dir() { "feature-first" } else { "flat" }
}

// ───────────── plugin (officiële Flutter/Dart-kennis) ─────────────

#[derive(Debug, PartialEq)]
pub enum PluginOutcome {
    AlreadyInstalled,
    Installed,
    Failed(String),
}

pub fn plugin_enabled(dir: &Path) -> bool {
    std::fs::read_to_string(dir.join(".claude/settings.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .is_some_and(|v| v["enabledPlugins"][PLUGIN_ID] == true)
}

/// Installeert de officiële plugin op projectniveau via de `claude` CLI (achtergrond; mislukken is niet erg:
/// Studio's eigen skills en tools blijven werken). `source` is normaal `flutter/agent-plugins`.
pub fn ensure_plugin(claude: &Path, dir: &Path, source: &str) -> PluginOutcome {
    if plugin_enabled(dir) {
        return PluginOutcome::AlreadyInstalled;
    }
    let step = |args: &[&str]| proc::run(claude, args, Some(dir), Duration::from_secs(240));
    match step(&["plugin", "marketplace", "add", source, "--scope", "project"]) {
        Ok(o) if o.ok() => {}
        Ok(o) => return PluginOutcome::Failed(crate::logging::redact(o.combined().trim())),
        Err(e) => return PluginOutcome::Failed(e.to_string()),
    }
    match step(&["plugin", "install", PLUGIN_ID, "--scope", "project"]) {
        Ok(o) if o.ok() => PluginOutcome::Installed,
        Ok(o) => PluginOutcome::Failed(crate::logging::redact(o.combined().trim())),
        Err(e) => PluginOutcome::Failed(e.to_string()),
    }
}

// ───────────── Dart-MCP ─────────────

/// MCP-serverdefinitie voor Dart/Flutter (`dart mcp-server`). Wordt per run in de `--mcp-config`
/// gezet, want `--strict-mcp-config` laat de server van de plugin anders vallen (zo getest).
pub fn dart_mcp_server() -> Value {
    json!({ "command": "dart", "args": ["mcp-server"], "env": { "AGENT_PLUGIN": "claude-code" } })
}

pub fn dart_version() -> Option<String> {
    let dart = proc::find("dart")?;
    let o = proc::run(&dart, &["--version"], None, Duration::from_secs(20)).ok()?;
    let v = crate::toolchain::parse_version(&o.combined())?;
    Some(format!("{}.{}.{}", v.0, v.1, v.2))
}

pub fn dart_mcp_supported(version: &str) -> bool {
    crate::toolchain::is_at_least(version, MIN_DART)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("studio-profile-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(d.join("lib/features/home")).unwrap();
        d
    }
    fn facts() -> ProjectFacts<'static> {
        ProjectFacts { name: "Demo", structure: "feature-first" }
    }
    fn read(d: &Path, rel: &str) -> String {
        std::fs::read_to_string(d.join(rel)).unwrap()
    }

    #[test]
    fn fresh_project_gets_the_whole_profile() {
        let d = tmp();
        let r = apply(&d, &facts()).unwrap();
        for f in [
            "CLAUDE.md", ".claude/settings.json", ".claude/rules/flutter.md", ".claude/hooks/studio_format.dart",
            ".claude/skills/studio-screen/SKILL.md", ".claude/skills/studio-theme/SKILL.md", ".claude/skills/studio-ui-kit/SKILL.md",
            ".claude/skills/studio-data-auth/SKILL.md", ".claude/skills/studio-release/SKILL.md", ".claude/skills/studio-store-listing/SKILL.md",
            ".studio/profile.json", "lib/features/home/CLAUDE.md",
        ] {
            assert!(d.join(f).is_file(), "missing {f}");
        }
        assert!(r.skipped.is_empty() && r.version == CURRENT);
        assert_eq!(load_manifest(&d).version, CURRENT);
    }

    #[test]
    fn claude_md_stays_short_and_only_has_what_code_cannot_tell() {
        let d = tmp();
        apply(&d, &facts()).unwrap();
        let md = read(&d, "CLAUDE.md");
        assert!(md.lines().count() < 40, "{} lines", md.lines().count());
        for must in ["STUDIO.md", "Riverpod", "go_router", "lib/core/", "lib/features/", "list_screens", "AppSpacing"] {
            assert!(md.contains(must), "{must}");
        }
        assert!(md.contains("breid_bereik_uit") && md.contains("scherm_index"), "profile v2 knows the scope tool and the index");
        assert!(!claude_block(&facts(), false).contains("breid_bereik_uit"));
        assert!(!md.contains("__"), "no unfilled placeholders");
    }

    #[test]
    fn applying_twice_changes_nothing() {
        let d = tmp();
        apply(&d, &facts()).unwrap();
        let before: Vec<(String, String)> = [".claude/settings.json", "CLAUDE.md", ".claude/rules/flutter.md"].iter().map(|f| (f.to_string(), read(&d, f))).collect();
        let r = apply(&d, &facts()).unwrap();
        assert!(r.written.is_empty() && r.updated.is_empty() && r.skipped.is_empty(), "{r:?}");
        for (f, text) in before {
            assert_eq!(read(&d, &f), text, "{f}");
        }
    }

    #[test]
    fn user_modified_studio_files_are_never_overwritten() {
        let d = tmp();
        apply(&d, &facts()).unwrap();
        std::fs::write(d.join(".claude/rules/flutter.md"), "my own rules\n").unwrap();
        let r = apply(&d, &facts()).unwrap();
        assert_eq!(read(&d, ".claude/rules/flutter.md"), "my own rules\n");
        assert!(r.skipped.iter().any(|(p, _)| p == ".claude/rules/flutter.md"));
    }

    #[test]
    fn unmodified_studio_files_are_updated_on_migration() {
        let d = tmp();
        apply(&d, &facts()).unwrap();
        // Simuleer een oudere profielversie: bestand met oude inhoud, waarvan het manifest de hash kent.
        let old = "# old rules\n";
        std::fs::write(d.join(".claude/rules/flutter.md"), old).unwrap();
        let mut m = load_manifest(&d);
        m.files.insert(".claude/rules/flutter.md".into(), hash(old));
        save_manifest(&d, &m).unwrap();
        let r = apply(&d, &facts()).unwrap();
        assert!(r.updated.contains(&".claude/rules/flutter.md".to_string()));
        assert!(read(&d, ".claude/rules/flutter.md").contains("Riverpod"));
    }

    #[test]
    fn claude_md_user_text_is_preserved_and_block_is_replaced_in_place() {
        let d = tmp();
        std::fs::write(d.join("CLAUDE.md"), "# Notes from the owner\nAlways answer in Dutch.\n").unwrap();
        apply(&d, &facts()).unwrap();
        let md = read(&d, "CLAUDE.md");
        assert!(md.starts_with("# Notes from the owner") && md.contains("Always answer in Dutch.") && md.contains(BLOCK_START));
        // een nieuwe versie van het blok vervangt alleen het blok
        let tweaked = md.replace("Explain in plain language", "OLD TEXT");
        std::fs::write(d.join("CLAUDE.md"), &tweaked).unwrap();
        apply(&d, &facts()).unwrap();
        let again = read(&d, "CLAUDE.md");
        assert!(again.contains("Explain in plain language") && !again.contains("OLD TEXT") && again.contains("Always answer in Dutch."));
        assert_eq!(again.matches(BLOCK_START).count(), 1);
    }

    #[test]
    fn a_v1_project_migrates_to_v2_without_touching_user_text_or_modified_files() {
        let d = tmp();
        // Zo zag een project uit fase 6 eruit: blok zonder de scope-regel, eigen tekst eromheen, aangepaste regels.
        let v1 = format!("My own notes\n\n{}\n\nMore notes\n", claude_block(&facts(), false).trim_end());
        std::fs::write(d.join("CLAUDE.md"), &v1).unwrap();
        apply(&d, &facts()).unwrap();
        std::fs::write(d.join(".claude/rules/flutter.md"), "my own rules\n").unwrap();
        let before = read(&d, "CLAUDE.md");
        assert!(before.contains("breid_bereik_uit") && before.contains("scherm_index"), "block updated to v2");
        assert!(before.starts_with("My own notes") && before.contains("More notes"), "text around the block is untouched");
        assert_eq!(before.matches("studio:start").count(), 1, "replaced in place, not appended");
        let r = apply(&d, &facts()).unwrap();
        assert_eq!(read(&d, "CLAUDE.md"), before);
        assert_eq!(read(&d, ".claude/rules/flutter.md"), "my own rules\n", "a file the user changed is never overwritten");
        assert!(r.skipped.iter().any(|s| s.0.contains("flutter.md")), "{:?}", r.skipped);
        assert_eq!(r.version, 2);
    }

    #[test]
    fn legacy_claude_md_from_earlier_studio_versions_is_replaced() {
        let d = tmp();
        std::fs::write(d.join("CLAUDE.md"), LEGACY_CLAUDE_MD).unwrap();
        apply(&d, &facts()).unwrap();
        assert!(!read(&d, "CLAUDE.md").contains("# Instructions for Claude"));
    }

    #[test]
    fn settings_merge_keeps_user_keys_and_is_idempotent() {
        let existing = r#"{"model":"opus","permissions":{"allow":["Bash(npm test)"],"deny":["Read(secrets/**)"]},
            "hooks":{"PostToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"echo hi"}]}]},
            "enabledPlugins":{"dart-flutter@dart-flutter":true}}"#;
        let merged = merge_settings(existing).unwrap();
        let v: Value = serde_json::from_str(&merged).unwrap();
        assert_eq!(v["model"], "opus");
        assert_eq!(v["permissions"]["allow"][0], "Bash(npm test)");
        let deny: Vec<&str> = v["permissions"]["deny"].as_array().unwrap().iter().map(|x| x.as_str().unwrap()).collect();
        assert!(deny.contains(&"Read(secrets/**)") && deny.contains(&"Read(build/**)") && deny.contains(&"Read(*.g.dart)") && deny.contains(&"Read(.dart_tool/**)"));
        assert_eq!(v["enabledPlugins"]["dart-flutter@dart-flutter"], true, "the plugin keys written by the claude CLI survive");
        assert_eq!(v["hooks"]["PostToolUse"].as_array().unwrap().len(), 2, "user hook + ours");
        assert_eq!(merge_settings(&merged).unwrap(), merged, "second merge is a no-op");
        assert!(merge_settings("{not json").is_err());
        assert!(merge_settings("[1]").is_err());
    }

    #[test]
    fn a_changed_studio_hook_replaces_the_old_one_instead_of_duplicating() {
        let old = r#"{"hooks":{"PostToolUse":[{"matcher":"Edit","hooks":[{"type":"command","command":"dart old/studio_format.dart"}]}]}}"#;
        let v: Value = serde_json::from_str(&merge_settings(old).unwrap()).unwrap();
        let arr = v["hooks"]["PostToolUse"].as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert!(arr[0]["hooks"][0]["command"].as_str().unwrap().contains("CLAUDE_PROJECT_DIR"));
    }

    #[test]
    fn invalid_settings_json_is_left_alone_and_reported() {
        let d = tmp();
        std::fs::create_dir_all(d.join(".claude")).unwrap();
        std::fs::write(d.join(".claude/settings.json"), "{broken").unwrap();
        let r = apply(&d, &facts()).unwrap();
        assert_eq!(read(&d, ".claude/settings.json"), "{broken");
        assert!(r.skipped.iter().any(|(p, _)| p == ".claude/settings.json"));
    }

    #[test]
    fn skills_have_short_bodies_and_precise_descriptions() {
        for f in owned_files().iter().filter(|f| f.path.ends_with("SKILL.md")) {
            let name = f.path.split('/').nth(2).unwrap();
            assert!(f.contents.starts_with(&format!("---\nname: {name}\ndescription: ")), "{}", f.path);
            let desc = f.contents.lines().nth(2).unwrap();
            assert!(desc.contains("Use ") && desc.len() > 80 && desc.len() < 420, "{}: description should say when to use it", f.path);
            assert!(f.contents.lines().count() < 25, "{} should stay short; details go in references/", f.path);
            assert!(f.contents.contains("references/") || name == "studio-theme" || name == "studio-release", "{}", f.path);
        }
        let refs = owned_files().iter().filter(|f| f.path.contains("/references/")).count();
        assert_eq!(refs, 6);
    }

    #[test]
    fn existing_features_get_local_rules_but_never_lose_their_own() {
        let d = tmp();
        std::fs::create_dir_all(d.join("lib/features/settings")).unwrap();
        std::fs::write(d.join("lib/features/settings/CLAUDE.md"), "# my rules\n").unwrap();
        apply(&d, &facts()).unwrap();
        assert!(read(&d, "lib/features/home/CLAUDE.md").lines().count() <= 6);
        assert_eq!(read(&d, "lib/features/settings/CLAUDE.md"), "# my rules\n");
    }

    #[test]
    fn flat_legacy_projects_are_told_not_to_reorganize() {
        let legacy = ProjectFacts { name: "Old", structure: "flat" };
        let b = claude_block(&legacy, false);
        assert!(b.contains("older flat structure") && b.lines().count() < 40);
        assert!(!claude_block(&facts(), false).contains("older flat structure"));
    }

    #[test]
    fn migrating_a_phase_1_to_5_project_leaves_its_code_alone() {
        let d = std::env::temp_dir().join(format!("studio-legacy-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(d.join("lib")).unwrap();
        std::fs::write(d.join("lib/main.dart"), "// the owner's flat app\nvoid main() {}\n").unwrap();
        std::fs::write(d.join("lib/app_theme.dart"), "// generated theme\n").unwrap();
        std::fs::write(d.join("CLAUDE.md"), LEGACY_CLAUDE_MD).unwrap();
        std::fs::write(d.join("STUDIO.md"), "# Old app\nmemory the owner cares about\n").unwrap();
        let facts = ProjectFacts { name: "Old", structure: structure_of(&d) };
        assert_eq!(facts.structure, "flat");
        let r = apply(&d, &facts).unwrap();
        assert!(r.skipped.is_empty(), "{r:?}");
        assert_eq!(read(&d, "lib/main.dart"), "// the owner's flat app\nvoid main() {}\n");
        assert_eq!(read(&d, "lib/app_theme.dart"), "// generated theme\n");
        assert_eq!(read(&d, "STUDIO.md"), "# Old app\nmemory the owner cares about\n", "project memory is never touched");
        let md = read(&d, "CLAUDE.md");
        assert!(md.contains("older flat structure") && !md.contains("# Instructions for Claude"));
        assert!(!d.join("lib/core").exists() && !d.join("lib/features").exists(), "no code is moved");
        // en een tweede migratie (bv. profiel v2 later) is veilig
        assert!(apply(&d, &facts).unwrap().updated.is_empty());
    }

    #[test]
    fn structure_detection() {
        let d = tmp();
        assert_eq!(structure_of(&d), "flat");
        std::fs::create_dir_all(d.join("lib/core")).unwrap();
        assert_eq!(structure_of(&d), "feature-first");
    }

    #[test]
    fn dart_requirement() {
        assert!(dart_mcp_supported("3.9.0") && dart_mcp_supported("3.14.0") && !dart_mcp_supported("3.8.9"));
        assert_eq!(dart_mcp_server()["args"][0], "mcp-server");
    }

    #[cfg(unix)]
    #[test]
    fn plugin_install_uses_the_official_commands_at_project_scope() {
        use std::os::unix::fs::PermissionsExt;
        let d = tmp();
        let fake = d.join("claude");
        std::fs::write(&fake, "#!/bin/sh\necho \"$@\" >> \"$(dirname \"$0\")/calls.txt\"\nif [ \"$2\" = install ]; then mkdir -p .claude; echo '{\"enabledPlugins\":{\"dart-flutter@dart-flutter\":true}}' > .claude/settings.json; fi\n").unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(ensure_plugin(&fake, &d, PLUGIN_SOURCE), PluginOutcome::Installed);
        let calls = read(&d, "calls.txt");
        assert!(calls.contains("plugin marketplace add flutter/agent-plugins --scope project") && calls.contains("plugin install dart-flutter@dart-flutter --scope project"), "{calls}");
        // tweede keer: niets meer doen
        assert_eq!(ensure_plugin(&fake, &d, PLUGIN_SOURCE), PluginOutcome::AlreadyInstalled);
        assert_eq!(read(&d, "calls.txt").lines().count(), 2);
        // en het profiel behoudt daarna de plugin-sleutels
        apply(&d, &facts()).unwrap();
        assert!(plugin_enabled(&d));
        // mislukken is netjes
        std::fs::remove_file(d.join(".claude/settings.json")).unwrap();
        std::fs::write(&fake, "#!/bin/sh\necho 'github unreachable' >&2\nexit 1\n").unwrap();
        assert!(matches!(ensure_plugin(&fake, &d, PLUGIN_SOURCE), PluginOutcome::Failed(m) if m.contains("github unreachable")));
    }
}

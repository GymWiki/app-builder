//! Projecten aanmaken (flutter create + sjabloon + git + STUDIO.md) en veilig verwijderen.

use crate::db::{Db, NewProject, Project};
use crate::error::{AppError, AppResult};
use crate::proc;
use std::path::{Path, PathBuf};
use std::time::Duration;

const MAX_NAME: usize = 60;

// Dart-trefwoorden die niet als pakketnaam mogen worden gebruikt.
const DART_RESERVED: &[&str] = &[
    "abstract", "as", "assert", "async", "await", "break", "case", "catch", "class", "const", "continue",
    "default", "do", "dynamic", "else", "enum", "export", "extends", "extension", "external", "factory",
    "false", "final", "finally", "for", "get", "if", "implements", "import", "in", "interface", "is", "late",
    "library", "mixin", "new", "null", "on", "operator", "part", "required", "rethrow", "return", "set",
    "show", "static", "super", "switch", "sync", "this", "throw", "true", "try", "typedef", "var", "void",
    "while", "with", "yield", "test", "flutter",
];

/// Maakt van een mensnaam een geldige Dart-pakketnaam (`snake_case`, begint met een letter).
pub fn slugify(name: &str) -> String {
    let mut s = String::new();
    for ch in name.trim().chars() {
        let c = fold_char(ch);
        if c.is_ascii_alphanumeric() {
            s.push(c.to_ascii_lowercase());
        } else if !s.ends_with('_') && !s.is_empty() {
            s.push('_');
        }
    }
    let mut s = s.trim_matches('_').to_string();
    if s.chars().next().is_none_or(|c| !c.is_ascii_lowercase()) {
        s = format!("app_{s}").trim_end_matches('_').to_string();
    }
    s.truncate(40);
    let mut s = s.trim_end_matches('_').to_string();
    if DART_RESERVED.contains(&s.as_str()) {
        s.push_str("_app");
    }
    s
}

fn fold_char(c: char) -> char {
    match c {
        'à'..='å' | 'À'..='Å' => 'a',
        'ç' | 'Ç' => 'c',
        'è'..='ë' | 'È'..='Ë' => 'e',
        'ì'..='ï' | 'Ì'..='Ï' => 'i',
        'ñ' | 'Ñ' => 'n',
        'ò'..='ö' | 'Ò'..='Ö' => 'o',
        'ù'..='ü' | 'Ù'..='Ü' => 'u',
        'ý' | 'ÿ' | 'Ý' => 'y',
        other => other,
    }
}

pub fn valid_org(org: &str) -> bool {
    !org.is_empty()
        && org.len() <= 60
        && org.split('.').count() >= 2
        && org.split('.').all(|seg| {
            seg.chars().next().is_some_and(|c| c.is_ascii_lowercase())
                && seg.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        })
}

pub fn bundle_id(org: &str, slug: &str) -> String {
    format!("{org}.{}", slug.replace('_', ""))
}

/// Projectgeheugen: wordt door Claude gelezen (via CLAUDE.md) en door Studio bijgehouden.
pub fn studio_md(name: &str, template: &str, description: &str, bundle: &str) -> String {
    let what = if description.trim().is_empty() {
        "_Not described yet. Ask the owner what the app should do and write it here._".to_string()
    } else {
        description.trim().to_string()
    };
    format!(
        "# {name}\n\n\
         > Project memory. Studio and Claude keep this file up to date. Read it before making changes.\n\n\
         ## What the app does\n\n{what}\n\n\
         ## Facts\n\n\
         - Framework: Flutter (Material 3), targets Android and iOS\n\
         - State: Riverpod (flutter_riverpod). Navigation: go_router. Structure: `lib/core/` (theme, models, navigation, shared widgets) and `lib/features/<screen>/`\n\
         - Starting template: {template}\n\
         - Bundle / application ID: `{bundle}`\n\n\
         ## Decisions\n\n\
         _Record choices here as they are made (navigation, data storage, colors, packages) with a short reason._\n\n\
         ## Working agreements\n\n\
         - The owner is not a programmer. Explain in plain language, keep changes small and focused.\n\
         - Keep all code inside `lib/` and tests inside `test/`. Prefer built-in Flutter widgets; add a package only when it clearly saves effort.\n\
         - After finishing a task, add at most 3 lines about it under Decisions (what changed and why).\n\
         - After every change the app must still build and `flutter analyze` must stay clean.\n\
         - Never put secrets (API keys, passwords, signing keys) in the project. They live in the OS keychain.\n\
         - Update this file when the app's behavior or an important decision changes.\n"
    )
}

/// Welke van de gewenste `flutter create`-opties bestaan echt in deze Flutter-versie?
/// (Harde regel 3: geen aannames over CLI-flags.)
pub fn supported_flags<'a>(help_text: &str, wanted: &[&'a str]) -> Vec<&'a str> {
    wanted.iter().copied().filter(|f| help_text.contains(f)).collect()
}

pub struct Tools {
    pub flutter: PathBuf,
    pub git: PathBuf,
}

pub struct CreateRequest {
    pub name: String,
    pub template: String,
    pub description: String,
    pub org: String,
}

fn unique_dir(base: &Path, slug: &str, db: &Db) -> AppResult<PathBuf> {
    for n in 1..100 {
        let dir = base.join(if n == 1 { slug.to_string() } else { format!("{slug}-{n}") });
        if !dir.exists() && !db.project_exists_at(&dir.to_string_lossy())? {
            return Ok(dir);
        }
    }
    Err(AppError::PathExists(base.join(slug).display().to_string()))
}

fn git(tools: &Tools, dir: &Path, args: &[&str]) -> AppResult<proc::Output> {
    let o = proc::run(&tools.git, args, Some(dir), Duration::from_secs(120))?;
    if !o.ok() {
        return Err(AppError::CommandFailed(format!("git {}: {}", args.join(" "), o.combined().trim())));
    }
    Ok(o)
}

fn ensure_git_identity(tools: &Tools, dir: &Path) -> AppResult<()> {
    for (key, fallback) in [("user.name", "Studio"), ("user.email", "studio@localhost")] {
        let o = proc::run(&tools.git, &["config", key], Some(dir), Duration::from_secs(10))?;
        if o.stdout.trim().is_empty() {
            git(tools, dir, &["config", key, fallback])?;
        }
    }
    Ok(())
}

pub fn create_project(
    db: &Db,
    tools: &Tools,
    projects_dir: &Path,
    req: &CreateRequest,
    progress: &dyn Fn(&str),
) -> AppResult<Project> {
    let name = req.name.trim();
    if name.is_empty() || name.chars().count() > MAX_NAME {
        return Err(AppError::Invalid("project name must be 1-60 characters".into()));
    }
    if crate::templates::files(&req.template).is_none() {
        return Err(AppError::Invalid(format!("unknown template {}", req.template)));
    }
    if !valid_org(&req.org) {
        return Err(AppError::Invalid("organisation id must look like com.example".into()));
    }
    progress("prepare");
    std::fs::create_dir_all(projects_dir)?;
    let slug = slugify(name);
    let dir = unique_dir(projects_dir, &slug, db)?;

    let result = (|| -> AppResult<Project> {
        // 1. flutter create — alleen flags gebruiken die deze versie kent.
        progress("flutter_create");
        let help = proc::run(&tools.flutter, &["create", "--help"], None, Duration::from_secs(120))?;
        let flags = supported_flags(&help.combined(), &["--platforms", "--org", "--project-name"]);
        let mut args: Vec<String> = vec!["create".into()];
        if flags.contains(&"--project-name") {
            args.push(format!("--project-name={slug}"));
        }
        if flags.contains(&"--org") {
            args.push(format!("--org={}", req.org));
        }
        if flags.contains(&"--platforms") {
            args.push("--platforms=android,ios,web".into());
        }
        args.push(dir.to_string_lossy().into_owned());
        let argv: Vec<&str> = args.iter().map(String::as_str).collect();
        let o = proc::run(&tools.flutter, &argv, Some(projects_dir), Duration::from_secs(900))?;
        if !o.ok() {
            return Err(AppError::CommandFailed(format!("flutter create: {}", o.combined().trim())));
        }

        // 2. sjabloon
        progress("template");
        // Vaste packages voor de gekozen state-/navigatie-oplossing (Riverpod + go_router).
        let mut add = vec!["pub", "add"];
        add.extend(crate::templates::PACKAGES);
        let o = proc::run(&tools.flutter, &add, Some(&dir), Duration::from_secs(600))?;
        if !o.ok() {
            return Err(AppError::CommandFailed(format!("flutter pub add: {}", o.combined().trim())));
        }
        std::fs::create_dir_all(dir.join("test"))?;
        crate::templates::write_all(&dir, &req.template, name)?;
        crate::studio_tools::write_theme(&dir, crate::studio_tools::DEFAULT_SEED)?;
        std::fs::write(dir.join("test/widget_test.dart"), crate::templates::smoke_test(&slug))?;

        // 3. projectgeheugen
        progress("memory");
        let bundle = bundle_id(&req.org, &slug);
        std::fs::write(dir.join("STUDIO.md"), studio_md(name, &req.template, &req.description, &bundle))?;
        crate::profile::apply(&dir, &crate::profile::ProjectFacts { name, structure: "feature-first" })?;
        let mut ignore = std::fs::read_to_string(dir.join(".gitignore")).unwrap_or_default();
        if !ignore.ends_with('\n') && !ignore.is_empty() {
            ignore.push('\n');
        }
        ignore.push_str("\n# Studio / Claude Code (local only)\n.claude/settings.local.json\n");
        std::fs::write(dir.join(".gitignore"), ignore)?;
        crate::leak::ensure_gitignore(&dir)?;

        // 4. git
        progress("git");
        git(tools, &dir, &["init"])?;
        git(tools, &dir, &["symbolic-ref", "HEAD", "refs/heads/main"])?;
        ensure_git_identity(tools, &dir)?;
        git(tools, &dir, &["add", "-A"])?;
        git(tools, &dir, &["commit", "-m", "Start: project created by Studio"])?;
        let commit = git(tools, &dir, &["rev-parse", "HEAD"])?.stdout.trim().to_string();

        // 5. opslaan
        progress("save");
        db.project_insert(&NewProject {
            name: name.to_string(),
            slug: slug.clone(),
            path: dir.to_string_lossy().into_owned(),
            template: req.template.clone(),
            description: req.description.trim().to_string(),
            bundle_id: bundle,
            initial_commit: Some(commit),
            profile_version: crate::profile::CURRENT,
        })
    })();

    if result.is_err() && dir.exists() {
        // Alleen opruimen wat deze aanroep zelf heeft aangemaakt (de map bestond nog niet).
        let _ = std::fs::remove_dir_all(&dir);
    }
    result
}

/// Verwijdert de projectmap alleen als die echt een Studio-project is en binnen de projectenmap ligt.
pub fn delete_project_files(projects_dir: &Path, project_path: &Path) -> AppResult<()> {
    let root = projects_dir.canonicalize()?;
    let target = match project_path.canonicalize() {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    if target == root || !target.starts_with(&root) {
        return Err(AppError::Unsafe(format!("{} is not inside the projects folder", target.display())));
    }
    if !target.join("STUDIO.md").is_file() {
        return Err(AppError::Unsafe(format!("{} does not look like a Studio project", target.display())));
    }
    std::fs::remove_dir_all(target)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_are_valid_dart_package_names() {
        assert_eq!(slugify("Mijn Eerste App!"), "mijn_eerste_app");
        assert_eq!(slugify("  Café — Scorebord  "), "cafe_scorebord");
        assert_eq!(slugify("123 Go"), "app_123_go");
        assert_eq!(slugify("class"), "class_app");
        assert_eq!(slugify("!!!"), "app");
        assert!(slugify(&"x".repeat(100)).len() <= 40);
    }

    #[test]
    fn org_and_bundle_validation() {
        assert!(valid_org("com.example"));
        assert!(valid_org("nl.mijnbedrijf.apps"));
        assert!(!valid_org("example"));
        assert!(!valid_org("com.Example"));
        assert!(!valid_org("com.1abc"));
        assert_eq!(bundle_id("com.example", "mijn_app"), "com.example.mijnapp");
    }

    #[test]
    fn studio_md_mentions_facts() {
        let md = studio_md("Demo", "tracker", "Counts water", "com.example.demo");
        assert!(md.contains("# Demo") && md.contains("Counts water") && md.contains("com.example.demo"));
        assert!(studio_md("Demo", "blank", "  ", "x").contains("Not described yet"));
    }

    #[test]
    fn only_known_flags_are_used() {
        let help = "Usage: flutter create\n  --org  --project-name  --platforms";
        assert_eq!(supported_flags(help, &["--platforms", "--org", "--nope"]), vec!["--platforms", "--org"]);
    }

    #[test]
    fn delete_refuses_foreign_paths() {
        let tmp = std::env::temp_dir().join(format!("studio-del-{}", uuid::Uuid::new_v4()));
        let root = tmp.join("projects");
        std::fs::create_dir_all(root.join("a")).unwrap();
        std::fs::create_dir_all(tmp.join("outside")).unwrap();
        std::fs::write(tmp.join("outside/STUDIO.md"), "x").unwrap();
        // geen STUDIO.md → geweigerd
        assert!(matches!(delete_project_files(&root, &root.join("a")), Err(AppError::Unsafe(_))));
        // buiten de projectenmap → geweigerd
        assert!(matches!(delete_project_files(&root, &tmp.join("outside")), Err(AppError::Unsafe(_))));
        // de projectenmap zelf → geweigerd
        assert!(matches!(delete_project_files(&root, &root), Err(AppError::Unsafe(_))));
        // geldig project → verwijderd
        std::fs::write(root.join("a/STUDIO.md"), "x").unwrap();
        delete_project_files(&root, &root.join("a")).unwrap();
        assert!(!root.join("a").exists());
        std::fs::remove_dir_all(tmp).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn full_creation_flow_with_fake_flutter() {
        use std::os::unix::fs::PermissionsExt;
        let Some(git_path) = proc::find("git") else { return };
        let tmp = std::env::temp_dir().join(format!("studio-create-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&tmp).unwrap();
        let fake = tmp.join("flutter");
        std::fs::write(
            &fake,
            "#!/bin/sh\n\
             if [ \"$2\" = \"--help\" ]; then echo '--org --project-name --platforms'; exit 0; fi\n\
             if [ \"$1\" = pub ]; then echo \"$@\" >> \"$(dirname \"$0\")/pub-add.log\"; exit 0; fi\n\
             for a; do dir=\"$a\"; done\n\
             mkdir -p \"$dir/lib\" \"$dir/test\"\n\
             echo '// default' > \"$dir/lib/main.dart\"\n\
             printf 'build/\\n' > \"$dir/.gitignore\"\n\
             echo 'name: x' > \"$dir/pubspec.yaml\"\n",
        )
        .unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();

        let db = Db::open_in_memory().unwrap();
        let tools = Tools { flutter: fake, git: git_path };
        let steps = std::cell::RefCell::new(Vec::new());
        let req = CreateRequest {
            name: "Mijn Scorebord".into(),
            template: "scoreboard".into(),
            description: "Scores bijhouden".into(),
            org: "com.example".into(),
        };
        let projects = tmp.join("projects");
        let p = create_project(&db, &tools, &projects, &req, &|s| steps.borrow_mut().push(s.to_string())).unwrap();

        assert_eq!(p.slug, "mijn_scorebord");
        assert_eq!(p.bundle_id, "com.example.mijnscorebord");
        assert_eq!(steps.borrow().as_slice(), ["prepare", "flutter_create", "template", "memory", "git", "save"]);
        let dir = Path::new(&p.path);
        assert!(dir.join("lib/core/theme/app_theme.dart").is_file() && dir.join(".studio/theme.json").is_file());
        assert!(!dir.join("lib/app_theme.dart").exists(), "feature-first projects keep the theme in lib/core");
        assert!(dir.join(".git").is_dir() && dir.join("STUDIO.md").is_file() && dir.join("CLAUDE.md").is_file());
        assert!(std::fs::read_to_string(dir.join("lib/features/scoreboard/scoreboard_screen.dart")).unwrap().contains("ScoreboardScreen"));
        assert!(std::fs::read_to_string(dir.join("lib/core/navigation/app_router.dart")).unwrap().contains("ScoreboardScreen"));
        assert!(std::fs::read_to_string(dir.join("pub-add.log")).is_err(), "pub add runs in the project dir, not next to the script");
        let pubadd = std::fs::read_to_string(tmp.join("pub-add.log")).unwrap();
        assert!(pubadd.contains("pub add flutter_riverpod go_router"), "{pubadd}");
        // projectprofiel
        let claude = std::fs::read_to_string(dir.join("CLAUDE.md")).unwrap();
        assert!(claude.lines().count() < 40 && claude.contains("Mijn Scorebord"));
        for f in [".claude/settings.json", ".claude/rules/flutter.md", ".claude/skills/studio-screen/SKILL.md", ".claude/hooks/studio_format.dart", "lib/features/scoreboard/CLAUDE.md"] {
            assert!(dir.join(f).is_file(), "missing {f}");
        }
        assert_eq!(db.profile_version(&p.id).unwrap(), crate::profile::CURRENT);
        let tracked = proc::run(&tools.git, &["ls-files"], Some(dir), Duration::from_secs(10)).unwrap().stdout;
        assert!(tracked.contains(".claude/skills/studio-screen/SKILL.md") && tracked.contains("CLAUDE.md"), "the profile is committed so every snapshot restores it");
        assert!(std::fs::read_to_string(dir.join(".gitignore")).unwrap().contains("settings.local.json"));
        let log = proc::run(&tools.git, &["log", "--oneline"], Some(dir), Duration::from_secs(10)).unwrap();
        assert_eq!(log.stdout.lines().count(), 1);
        let status = proc::run(&tools.git, &["status", "--porcelain"], Some(dir), Duration::from_secs(10)).unwrap();
        assert!(status.stdout.trim().is_empty(), "working tree should be clean");

        // tweede project met dezelfde naam krijgt een eigen map
        let p2 = create_project(&db, &tools, &projects, &req, &|_| {}).unwrap();
        assert!(p2.path.ends_with("mijn_scorebord-2"));

        // mislukte flutter create ruimt de map op en laat niets in de database achter
        std::fs::write(&tools.flutter, "#!/bin/sh\nif [ \"$2\" = \"--help\" ]; then exit 0; fi\nfor a; do d=\"$a\"; done; mkdir -p \"$d\"; echo nope >&2; exit 1\n").unwrap();
        assert!(create_project(&db, &tools, &projects, &req, &|_| {}).is_err());
        assert!(!projects.join("mijn_scorebord-3").exists());
        assert_eq!(db.projects_list(None).unwrap().len(), 2);
        std::fs::remove_dir_all(tmp).unwrap();
    }

    /// Echte Flutter + git nodig: `cargo test real_flutter_templates -- --ignored --nocapture`
    /// Maakt elk sjabloon aan zoals Studio dat doet, en draait `flutter analyze` en `flutter test`.
    #[test]
    #[ignore]
    fn real_flutter_templates() {
        let (Some(flutter), Some(git)) = (proc::find("flutter"), proc::find("git")) else { panic!("flutter and git must be on PATH") };
        let root = std::env::temp_dir().join(format!("studio-real-{}", uuid::Uuid::new_v4()));
        let db = Db::open_in_memory().unwrap();
        let tools = Tools { flutter: flutter.clone(), git };
        for template in crate::templates::TEMPLATES {
            let req = CreateRequest { name: format!("Real {template}"), template: template.into(), description: String::new(), org: "com.example".into() };
            let p = create_project(&db, &tools, &root, &req, &|s| println!("[{template}] {s}")).unwrap();
            let dir = Path::new(&p.path);
            let analyze = proc::run(&flutter, &["analyze"], Some(dir), Duration::from_secs(600)).unwrap();
            println!("[{template}] analyze: {}", analyze.combined().lines().last().unwrap_or(""));
            assert!(analyze.ok() && analyze.combined().contains("No issues found"), "{template}: {}", analyze.combined());
            let test = proc::run(&flutter, &["test"], Some(dir), Duration::from_secs(600)).unwrap();
            println!("[{template}] test: {}", test.combined().lines().last().unwrap_or(""));
            assert!(test.ok(), "{template}: {}", test.combined());
            let screens = crate::studio_tools::list_screens(dir).unwrap();
            assert!(!screens.is_empty(), "{template}: list_screens finds the screen");
            let g = crate::screens::screen_graph(dir).unwrap();
            assert_eq!(g.home.as_deref(), Some(screens[0].class.as_str()), "{template}: the '/' route is the start screen");
        }
    }
}

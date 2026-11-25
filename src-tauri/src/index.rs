//! Lokale code-index (fase 7): wie importeert wie, welk scherm hoort bij welke bestanden en welke bestanden
//! worden gedeeld. Alles komt uit een snelle tekstscan van `lib/` (geen Dart-compiler nodig) en wordt bij elke
//! run opnieuw opgebouwd, dus er is nooit een verouderde index om te synchroniseren.

use crate::error::AppResult;
use crate::studio_tools::list_screens;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct FileNode {
    pub path: String,
    /// Projectbestanden die dit bestand direct importeert, exporteert of als `part` gebruikt.
    pub imports: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ScreenEntry {
    pub class: String,
    pub file: String,
    /// `lib/features/<naam>` als het scherm in die structuur staat.
    pub feature_dir: Option<String>,
    pub route: Option<String>,
    /// Bestanden van dit scherm zelf (de featuremap, of alleen het bestand bij een platte structuur).
    pub own: Vec<String>,
    /// Bestanden buiten het scherm die het (indirect) gebruikt, zoals thema, modellen en gedeelde widgets.
    pub shared: Vec<String>,
    /// Schermen waar dit scherm naartoe navigeert.
    pub links: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct CodeIndex {
    pub files: BTreeMap<String, FileNode>,
    pub screens: Vec<ScreenEntry>,
}

fn is_generated(rel: &str) -> bool {
    rel.ends_with(".g.dart") || rel.ends_with(".freezed.dart") || rel.ends_with(".gr.dart")
}

fn walk(dir: &Path, root: &Path, out: &mut Vec<String>, depth: usize) {
    if depth > 10 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, root, out, depth + 1);
        } else if p.extension().is_some_and(|x| x == "dart") {
            if let Ok(r) = p.strip_prefix(root) {
                let rel = r.to_string_lossy().replace('\\', "/");
                if !is_generated(&rel) {
                    out.push(rel);
                }
            }
        }
    }
}

fn package_name(dir: &Path) -> String {
    std::fs::read_to_string(dir.join("pubspec.yaml"))
        .ok()
        .and_then(|t| t.lines().find_map(|l| l.strip_prefix("name:").map(|n| n.trim().trim_matches(['"', '\'']).to_string())))
        .unwrap_or_default()
}

/// Het pad uit `import '...'`, `export '...'` of `part '...'`.
fn directive_target(line: &str) -> Option<&str> {
    let t = line.trim_start();
    let rest = ["import ", "export ", "part "].iter().find_map(|k| t.strip_prefix(k))?;
    let rest = rest.trim_start();
    let q = rest.chars().next().filter(|c| *c == '\'' || *c == '"')?;
    let inner = &rest[1..];
    Some(&inner[..inner.find(q)?])
}

fn normalize(path: &str) -> String {
    let mut parts: Vec<&str> = vec![];
    for p in path.split('/') {
        match p {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            x => parts.push(x),
        }
    }
    parts.join("/")
}

fn resolve(target: &str, from: &str, pkg: &str) -> Option<String> {
    if let Some(rest) = target.strip_prefix("package:") {
        let (p, path) = rest.split_once('/')?;
        return (p == pkg).then(|| format!("lib/{}", normalize(path)));
    }
    if target.contains(':') {
        return None; // dart:core, andere pakketten
    }
    let base = from.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
    Some(normalize(&format!("{base}/{target}")))
}

impl CodeIndex {
    pub fn build(dir: &Path) -> AppResult<Self> {
        let pkg = package_name(dir);
        let mut paths = vec![];
        walk(&dir.join("lib"), dir, &mut paths, 0);
        paths.sort();
        let known: BTreeSet<&str> = paths.iter().map(String::as_str).collect();
        let mut files = BTreeMap::new();
        for p in &paths {
            let text = std::fs::read_to_string(dir.join(p)).unwrap_or_default();
            let mut imports: Vec<String> = text
                .lines()
                .filter_map(directive_target)
                .filter_map(|t| resolve(t, p, &pkg))
                .filter(|r| known.contains(r.as_str()) && r != p)
                .collect();
            imports.sort();
            imports.dedup();
            files.insert(p.clone(), FileNode { path: p.clone(), imports });
        }
        let mut idx = CodeIndex { files, screens: vec![] };

        let screens = list_screens(dir)?;
        let graph = crate::screens::screen_graph(dir)?;
        let names: Vec<&str> = screens.iter().map(|s| s.class.as_str()).collect();
        let routes = crate::screens::route_map(dir, &names);
        let screen_files: BTreeSet<&str> = screens.iter().map(|s| s.file.as_str()).collect();
        for s in &screens {
            let feature_dir = s.file.strip_prefix("lib/features/").and_then(|r| r.split('/').next()).map(|f| format!("lib/features/{f}"));
            let own: Vec<String> = match &feature_dir {
                Some(fd) => idx.files.keys().filter(|k| k.starts_with(&format!("{fd}/"))).cloned().collect(),
                None => vec![s.file.clone()],
            };
            let own_set: BTreeSet<&str> = own.iter().map(String::as_str).collect();
            let shared: Vec<String> = idx
                .deps(&s.file)
                .into_iter()
                .filter(|d| !own_set.contains(d.as_str()) && !screen_files.contains(d.as_str()))
                .collect();
            let route = routes.iter().find(|(_, c)| **c == s.class).map(|(p, _)| format!("/{p}"));
            let links = graph.nodes.iter().find(|n| n.class == s.class).map(|n| n.children.clone()).unwrap_or_default();
            idx.screens.push(ScreenEntry { class: s.class.clone(), file: s.file.clone(), feature_dir, route, own, shared, links });
        }
        Ok(idx)
    }

    /// Alle bestanden die `file` direct of indirect nodig heeft.
    pub fn deps(&self, file: &str) -> BTreeSet<String> {
        let mut seen = BTreeSet::new();
        let mut stack = vec![file.to_string()];
        while let Some(f) = stack.pop() {
            if let Some(n) = self.files.get(&f) {
                for i in &n.imports {
                    if seen.insert(i.clone()) {
                        stack.push(i.clone());
                    }
                }
            }
        }
        seen.remove(file);
        seen
    }

    /// Alle bestanden die `file` direct of indirect gebruiken.
    pub fn dependents(&self, file: &str) -> BTreeSet<String> {
        let mut seen = BTreeSet::new();
        let mut stack = vec![file.to_string()];
        while let Some(f) = stack.pop() {
            for (p, n) in &self.files {
                if n.imports.contains(&f) && seen.insert(p.clone()) {
                    stack.push(p.clone());
                }
            }
        }
        seen.remove(file);
        seen
    }

    pub fn screen(&self, name_or_file: &str) -> Option<&ScreenEntry> {
        self.screens.iter().find(|s| s.class.eq_ignore_ascii_case(name_or_file) || s.file == name_or_file)
    }

    /// Schermen die door een wijziging aan `files` geraakt kunnen worden.
    pub fn screens_affected(&self, files: &[String]) -> Vec<String> {
        let mut hit = BTreeSet::new();
        for f in files {
            let mut all = self.dependents(f);
            all.insert(f.clone());
            for s in &self.screens {
                if all.contains(&s.file) || s.own.iter().any(|o| all.contains(o)) {
                    hit.insert(s.class.clone());
                }
            }
        }
        hit.into_iter().collect()
    }

    /// Korte, leesbare schermindex voor `.studio/screens.md` en de MCP-tool `scherm_index`.
    pub fn render_markdown(&self) -> String {
        let mut s = String::from("# Screen index\n\nGenerated by Studio before every run. Read this first; open files only when you need them.\n\n");
        if self.screens.is_empty() {
            s.push_str("_No screens yet._\n");
        }
        for sc in &self.screens {
            s.push_str(&format!("## {} — `{}`", sc.class, sc.file));
            if let Some(r) = &sc.route {
                s.push_str(&format!(" (route `{r}`)"));
            }
            s.push('\n');
            if let Some(fd) = &sc.feature_dir {
                s.push_str(&format!("- Own files: `{fd}/**` ({} files)\n", sc.own.len()));
            }
            if !sc.shared.is_empty() {
                s.push_str(&format!("- Shared (used, edit only with `breid_bereik_uit`): {}\n", sc.shared.iter().map(|x| format!("`{x}`")).collect::<Vec<_>>().join(", ")));
            }
            if !sc.links.is_empty() {
                s.push_str(&format!("- Navigates to: {}\n", sc.links.join(", ")));
            }
            s.push('\n');
        }
        s
    }
}

#[cfg(test)]
pub fn fixture(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("studio-index-{name}-{}", uuid::Uuid::new_v4()));
    let w = |p: &str, t: &str| {
        let f = d.join(p);
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(f, t).unwrap();
    };
    w("pubspec.yaml", "name: demo\n");
    w("lib/main.dart", "import 'package:demo/app.dart';\nvoid main() => runApp(const App());\n");
    w("lib/app.dart", "import 'package:flutter/material.dart';\nimport 'core/navigation/router.dart';\nimport 'core/theme/app_theme.dart';\nclass App extends StatelessWidget { Widget build(c) => MaterialApp.router(routerConfig: router); }\n");
    w(
        "lib/core/navigation/router.dart",
        "import 'package:demo/features/home/home_screen.dart';\nimport '../../features/settings/settings_screen.dart';\nfinal router = GoRouter(routes: [\n GoRoute(path: '/', builder: (c, s) => const HomeScreen()),\n GoRoute(path: '/settings', builder: (c, s) => const SettingsScreen()),\n]);\n",
    );
    w("lib/core/theme/app_theme.dart", "class AppTheme {}\n");
    w("lib/core/widgets/empty_state.dart", "import '../theme/app_theme.dart';\nclass EmptyState {}\n");
    w(
        "lib/features/home/home_screen.dart",
        "import 'package:demo/core/widgets/empty_state.dart';\nimport 'widgets/card.dart';\nclass HomeScreen extends StatelessWidget { Widget build(c) => TextButton(onPressed: () => context.go('/settings'), child: Text('s')); }\n",
    );
    w("lib/features/home/widgets/card.dart", "class HomeCard {}\n");
    w("lib/features/settings/settings_screen.dart", "import 'package:demo/core/theme/app_theme.dart';\nclass SettingsScreen extends StatelessWidget { Widget build(c) => Text('x'); }\n");
    w("lib/features/settings/model.g.dart", "// generated\n");
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_package_and_relative_imports_and_ignores_generated_and_foreign() {
        let d = fixture("imports");
        let idx = CodeIndex::build(&d).unwrap();
        assert_eq!(idx.files["lib/main.dart"].imports, vec!["lib/app.dart"]);
        assert_eq!(idx.files["lib/app.dart"].imports, vec!["lib/core/navigation/router.dart", "lib/core/theme/app_theme.dart"]);
        assert_eq!(idx.files["lib/core/widgets/empty_state.dart"].imports, vec!["lib/core/theme/app_theme.dart"]);
        assert!(!idx.files.contains_key("lib/features/settings/model.g.dart"));
        assert_eq!(directive_target("import 'dart:io';"), Some("dart:io"));
        assert_eq!(directive_target("  part \"a.dart\";"), Some("a.dart"));
        assert_eq!(directive_target("final x = 'import';"), None);
    }

    #[test]
    fn transitive_deps_and_dependents() {
        let d = fixture("graph");
        let idx = CodeIndex::build(&d).unwrap();
        let deps = idx.deps("lib/features/home/home_screen.dart");
        assert!(deps.contains("lib/core/theme/app_theme.dart"), "through empty_state");
        assert!(deps.contains("lib/features/home/widgets/card.dart"));
        let users = idx.dependents("lib/core/theme/app_theme.dart");
        for f in ["lib/core/widgets/empty_state.dart", "lib/features/home/home_screen.dart", "lib/features/settings/settings_screen.dart", "lib/app.dart", "lib/main.dart"] {
            assert!(users.contains(f), "{f} uses the theme (directly or indirectly)");
        }
        assert!(!users.contains("lib/features/home/widgets/card.dart"));
    }

    #[test]
    fn screen_entries_split_own_and_shared_files() {
        let d = fixture("screens");
        let idx = CodeIndex::build(&d).unwrap();
        let home = idx.screen("homescreen").unwrap();
        assert_eq!(home.feature_dir.as_deref(), Some("lib/features/home"));
        assert_eq!(home.own, vec!["lib/features/home/home_screen.dart", "lib/features/home/widgets/card.dart"]);
        assert_eq!(home.shared, vec!["lib/core/theme/app_theme.dart", "lib/core/widgets/empty_state.dart"]);
        assert_eq!(home.route.as_deref(), Some("/"));
        assert_eq!(home.links, vec!["SettingsScreen"]);
        let md = idx.render_markdown();
        assert!(md.contains("## HomeScreen") && md.contains("breid_bereik_uit"));
    }

    #[test]
    fn a_shared_file_affects_every_screen_that_uses_it() {
        let d = fixture("affected");
        let idx = CodeIndex::build(&d).unwrap();
        assert_eq!(idx.screens_affected(&["lib/core/theme/app_theme.dart".into()]), vec!["HomeScreen", "SettingsScreen"]);
        assert_eq!(idx.screens_affected(&["lib/features/home/widgets/card.dart".into()]), vec!["HomeScreen"]);
    }

    #[test]
    fn flat_projects_use_the_screen_file_as_its_own_scope() {
        let d = std::env::temp_dir().join(format!("studio-index-flat-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(d.join("lib")).unwrap();
        std::fs::write(d.join("pubspec.yaml"), "name: flat\n").unwrap();
        std::fs::write(d.join("lib/main.dart"), "import 'app_theme.dart';\nclass HomeScreen extends StatelessWidget { Widget build(c) => Text('x'); }\n").unwrap();
        std::fs::write(d.join("lib/app_theme.dart"), "class AppTheme {}\n").unwrap();
        let idx = CodeIndex::build(&d).unwrap();
        let s = idx.screen("HomeScreen").unwrap();
        assert_eq!(s.own, vec!["lib/main.dart"]);
        assert_eq!(s.shared, vec!["lib/app_theme.dart"]);
    }
}

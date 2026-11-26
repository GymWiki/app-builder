//! Schermenboom: welke schermen er zijn en hoe je ertussen navigeert, afgeleid uit de Dart-code.
//! Een verwijzing naar een ander scherm (`SettingsScreen(`) binnen de klasse van een scherm geldt
//! als navigatie-link; de beginscherm is wat `home:` of `initialRoute` gebruikt.

use crate::error::AppResult;
use crate::studio_tools::{list_screens, Screen};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Node {
    pub class: String,
    pub file: String,
    pub line: usize,
    pub children: Vec<String>,
    /// go_router-pad van dit scherm (voor de preview); leeg als het scherm geen eigen pad heeft.
    pub route: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ScreenGraph {
    pub home: Option<String>,
    pub nodes: Vec<Node>,
    /// Schermen die nergens naartoe gelinkt zijn (en niet het beginscherm).
    pub unlinked: Vec<String>,
}

/// Tekst van het blok `class Naam … { … }` (accolades tellen, strings/commentaar genegeerd).
fn class_body<'a>(text: &'a str, class: &str) -> Option<&'a str> {
    let needle = format!("class {class}");
    let mut from = 0;
    let start = loop {
        let i = text[from..].find(&needle)? + from;
        let after = text[i + needle.len()..].chars().next();
        if after.is_none_or(|c| !(c.is_alphanumeric() || c == '_')) {
            break i;
        }
        from = i + needle.len();
    };
    let open = text[start..].find('{')? + start;
    let mut depth = 0usize;
    for (off, ch) in text[open..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&text[open..open + off + 1]);
                }
            }
            _ => {}
        }
    }
    Some(&text[open..])
}

fn mentions(body: &str, class: &str) -> bool {
    let needle = format!("{class}(");
    body.match_indices(&needle).any(|(i, _)| i == 0 || !body[..i].chars().next_back().is_some_and(|c| c.is_alphanumeric() || c == '_'))
}

/// `home: const HomeScreen()` → `HomeScreen`
fn find_home(project_dir: &Path, known: &[&str]) -> Option<String> {
    let main = std::fs::read_to_string(project_dir.join("lib/main.dart")).ok()?;
    let idx = main.find("home:").or_else(|| main.find("initialRoute"))?;
    let tail = &main[idx..main.len().min(idx + 160)];
    known.iter().find(|k| mentions(tail, k)).map(|s| s.to_string())
}

fn all_dart(dir: &Path, out: &mut Vec<std::path::PathBuf>, depth: usize) {
    if depth > 8 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            all_dart(&p, out, depth + 1);
        } else if p.extension().is_some_and(|x| x == "dart") {
            out.push(p);
        }
    }
}

fn norm_route(p: &str) -> String {
    p.split(['?', '#']).next().unwrap_or("").trim_matches('/').to_string()
}

/// Alle tekst-literals (`'...'` of `"..."`) in een stuk code.
fn string_literals(text: &str) -> Vec<&str> {
    let b = text.as_bytes();
    let mut out = vec![];
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\'' || b[i] == b'"' {
            let q = b[i];
            let start = i + 1;
            let mut j = start;
            while j < b.len() && b[j] != q && b[j] != b'\n' {
                j += 1;
            }
            if j < b.len() && b[j] == q {
                out.push(&text[start..j]);
            }
            i = j + 1;
        } else {
            i += 1;
        }
    }
    out
}

/// go_router: `GoRoute(path: '/settings', builder: (c, s) => const SettingsScreen())` → pad → scherm.
pub fn route_map(project_dir: &Path, names: &[&str]) -> BTreeMap<String, String> {
    let mut files = vec![];
    all_dart(&project_dir.join("lib"), &mut files, 0);
    let mut map = BTreeMap::new();
    for f in files {
        let Ok(text) = std::fs::read_to_string(&f) else { continue };
        let mut from = 0;
        while let Some(i) = text[from..].find("GoRoute(") {
            let at = from + i;
            let end = text[at + 8..].find("GoRoute(").map(|n| at + 8 + n).unwrap_or(text.len()).min(at + 600);
            let span = &text[at..end];
            if let Some(pi) = span.find("path:") {
                if let Some(path) = string_literals(&span[pi..]).first() {
                    if let Some(screen) = names.iter().find(|n| mentions(span, n)) {
                        map.entry(norm_route(path)).or_insert_with(|| screen.to_string());
                    }
                }
            }
            from = at + 8;
        }
    }
    map
}

pub fn screen_graph(project_dir: &Path) -> AppResult<ScreenGraph> {
    let screens: Vec<Screen> = list_screens(project_dir)?;
    let names: Vec<&str> = screens.iter().map(|s| s.class.as_str()).collect();
    let routes = route_map(project_dir, &names);
    let mut nodes = vec![];
    for s in &screens {
        let text = std::fs::read_to_string(project_dir.join(&s.file)).unwrap_or_default();
        let body = class_body(&text, &s.class).unwrap_or("");
        // Een StatefulWidget-scherm doet zijn werk in de bijbehorende State-klasse.
        let state_body = class_body(&text, &format!("_{}State", s.class)).unwrap_or("");
        let mut children: Vec<String> = names
            .iter()
            .filter(|n| **n != s.class && (mentions(body, n) || mentions(state_body, n)))
            .map(|n| n.to_string())
            .collect();
        // Navigatie via go_router: `context.go('/settings')` in het scherm.
        for lit in string_literals(body).into_iter().chain(string_literals(state_body)) {
            if let Some(target) = routes.get(&norm_route(lit)) {
                if *target != s.class && !children.contains(target) && lit.starts_with('/') {
                    children.push(target.clone());
                }
            }
        }
        let route = routes.iter().find(|(_, v)| **v == s.class).map(|(k, _)| format!("/{}", k.trim_start_matches('/')));
        nodes.push(Node { class: s.class.clone(), file: s.file.clone(), line: s.line, children, route });
    }
    let home = routes.get("").cloned().or_else(|| find_home(project_dir, &names)).or_else(|| names.first().map(|s| s.to_string()));
    // Wie bereikbaar is vanaf home?
    let by_name: BTreeMap<&str, &Node> = nodes.iter().map(|n| (n.class.as_str(), n)).collect();
    let mut seen = BTreeSet::new();
    let mut stack: Vec<&str> = home.as_deref().into_iter().collect();
    while let Some(c) = stack.pop() {
        if seen.insert(c.to_string()) {
            if let Some(n) = by_name.get(c) {
                stack.extend(n.children.iter().map(String::as_str));
            }
        }
    }
    let unlinked = nodes.iter().filter(|n| !seen.contains(&n.class)).map(|n| n.class.clone()).collect();
    Ok(ScreenGraph { home, nodes, unlinked })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_navigation_tree() {
        let d = std::env::temp_dir().join(format!("studio-screens-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(d.join("lib")).unwrap();
        std::fs::write(
            d.join("lib/main.dart"),
            "void main() => runApp(const MyApp());\n\
             class MyApp extends StatelessWidget {\n  Widget build(c) => MaterialApp(home: const HomeScreen());\n}\n\
             class HomeScreen extends StatelessWidget {\n  Widget build(c) => ElevatedButton(onPressed: () => Navigator.push(c, MaterialPageRoute(builder: (_) => const SettingsScreen())), child: Text('x'));\n}\n\
             class SettingsScreen extends StatefulWidget {\n  State<SettingsScreen> createState() => _SettingsScreenState();\n}\n\
             class _SettingsScreenState extends State<SettingsScreen> {\n  Widget build(c) => TextButton(onPressed: () => Navigator.push(c, MaterialPageRoute(builder: (_) => AboutPage())), child: Text('y'));\n}\n\
             class AboutPage extends StatelessWidget { Widget build(c) => Text('about'); }\n\
             class LonelyScreen extends StatelessWidget { Widget build(c) => Text('alone'); }\n",
        )
        .unwrap();
        let g = screen_graph(&d).unwrap();
        assert_eq!(g.home.as_deref(), Some("HomeScreen"));
        let by = |n: &str| g.nodes.iter().find(|x| x.class == n).unwrap();
        assert_eq!(by("HomeScreen").children, vec!["SettingsScreen"]);
        assert_eq!(by("SettingsScreen").children, vec!["AboutPage"]);
        assert!(by("AboutPage").children.is_empty());
        assert_eq!(g.unlinked, vec!["LonelyScreen"]);
    }

    #[test]
    fn follows_go_router_routes() {
        let d = std::env::temp_dir().join(format!("studio-screens-gr-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(d.join("lib/core/navigation")).unwrap();
        std::fs::create_dir_all(d.join("lib/features/home")).unwrap();
        std::fs::create_dir_all(d.join("lib/features/settings")).unwrap();
        std::fs::write(
            d.join("lib/core/navigation/app_router.dart"),
            "final appRouter = GoRouter(routes: [\n  GoRoute(path: '/', builder: (c, s) => const HomeScreen()),\n  GoRoute(path: '/settings', builder: (c, s) => const SettingsScreen()),\n  GoRoute(path: '/about', builder: (c, s) => const AboutScreen()),\n]);\n",
        )
        .unwrap();
        std::fs::write(d.join("lib/features/home/home_screen.dart"), "class HomeScreen extends ConsumerWidget {\n  Widget build(c, ref) => TextButton(onPressed: () => context.go('/settings'), child: Text('s'));\n}\n").unwrap();
        std::fs::write(d.join("lib/features/settings/settings_screen.dart"), "class SettingsScreen extends StatelessWidget {\n  Widget build(c) => TextButton(onPressed: () => context.push(\"/about?from=settings\"), child: Text('a'));\n}\n").unwrap();
        std::fs::write(d.join("lib/features/settings/about_screen.dart"), "class AboutScreen extends StatelessWidget { Widget build(c) => Text('x'); }\n").unwrap();
        let g = screen_graph(&d).unwrap();
        assert_eq!(g.home.as_deref(), Some("HomeScreen"), "the '/' route is the start screen");
        let by = |n: &str| g.nodes.iter().find(|x| x.class == n).unwrap();
        assert_eq!(by("HomeScreen").children, vec!["SettingsScreen"]);
        assert_eq!(by("SettingsScreen").children, vec!["AboutScreen"]);
        assert!(g.unlinked.is_empty(), "{:?}", g.unlinked);
    }

    #[test]
    fn empty_project_has_no_home() {
        let d = std::env::temp_dir().join(format!("studio-screens-e-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(d.join("lib")).unwrap();
        let g = screen_graph(&d).unwrap();
        assert!(g.home.is_none() && g.nodes.is_empty());
    }
}

//! Het door de gebruiker gekozen stuk van een scherm, vertaald naar vaste feiten voor Claude, en de controle achteraf
//! of Claude daarbinnen is gebleven. De controle is een signaal, geen blokkade: afwijkingen worden gelogd (ontwikkelaarsmodus)
//! en het bestaande bereik-systeem (scope-router) beslist wat er verder gebeurt.

use serde::{Deserialize, Serialize};

/// Eén widget in de keten, van binnen naar buiten, alleen uit eigen code.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Link {
    pub widget: String,
    pub file: String,
    pub line: usize,
    pub column: usize,
}

/// Een gekozen widget met alles wat Claude moet weten.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    pub widget: String,
    /// Zichtbare tekst in of op de widget, als die er is.
    pub text: Option<String>,
    pub file: String,
    pub start_line: usize,
    pub end_line: usize,
    /// Van binnen naar buiten, eigen code (ook gedeelde componenten).
    pub chain: Vec<Link>,
    /// De widget staat in een gedeeld component: dit is de aanroeplocatie op het scherm.
    pub via_shared: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Selection {
    pub screen: Option<String>,
    pub screen_file: Option<String>,
    pub targets: Vec<Target>,
    /// Het getekende vak in procenten (x, y, breedte, hoogte).
    pub bbox: Option<[f64; 4]>,
}

impl Selection {
    /// Toegestane regelbereiken: de uitdrukking van elke gekozen widget (inclusief het bestand, zodat bijvoorbeeld imports
    /// bovenin niet meetellen als afwijking... zie `deviations`).
    pub fn ranges(&self) -> Vec<(String, usize, usize)> {
        self.targets.iter().map(|t| (t.file.clone(), t.start_line, t.end_line)).collect()
    }

    /// Vaste feiten voor Claude (Engels).
    pub fn prompt(&self) -> String {
        let mut s = String::from("The user marked part of the live preview and wants ONLY that part changed.\n");
        if let Some(sc) = &self.screen {
            s.push_str(&format!("Screen: {sc}{}.\n", self.screen_file.as_ref().map(|f| format!(" ({f})")).unwrap_or_default()));
        }
        s.push_str("Widgets inside the marked box (exact code locations from Flutter's widget inspector, current as of this request):\n");
        for (i, t) in self.targets.iter().enumerate() {
            s.push_str(&format!("{}. {}{} — {} lines {}–{}", i + 1, t.widget, t.text.as_ref().map(|x| format!(" \"{x}\"")).unwrap_or_default(), t.file, t.start_line, t.end_line));
            if t.via_shared {
                s.push_str(" (this is the place where a shared component is used; do not change the shared component itself unless the user asks for all uses)");
            }
            let chain: Vec<String> = t.chain.iter().skip(1).take(4).map(|l| format!("{} ({}:{})", l.widget, l.file, l.line)).collect();
            if !chain.is_empty() {
                s.push_str(&format!("; inside {}", chain.join(" < ")));
            }
            s.push('\n');
        }
        if let Some([x, y, w, h]) = self.bbox {
            s.push_str(&format!("Box on the phone screen: left {x:.0}%, top {y:.0}%, width {w:.0}%, height {h:.0}%.\n"));
        }
        s.push_str("Change only code inside those line ranges (and the minimal edits their parameters need). Leave the rest of the screen and file exactly as it is. If the request cannot be done within them, do not do it: explain what would have to change and ask the user to select more.");
        s
    }
}

/// Eén gewijzigd stuk uit `git diff -U0`: bestand en oude regels.
#[derive(Debug, Clone, PartialEq)]
pub struct Hunk {
    pub file: String,
    /// Eerste oude regel en aantal. Bij 0 regels is het een invoeging ná `old_start`.
    pub old_start: usize,
    pub old_len: usize,
}

pub fn parse_hunks(diff: &str) -> Vec<Hunk> {
    let mut out = vec![];
    let mut file = String::new();
    for l in diff.lines() {
        if let Some(f) = l.strip_prefix("+++ b/") {
            file = f.to_string();
        } else if l.starts_with("+++ /dev/null") {
            // verwijderd bestand: de `--- a/` regel hierboven had de naam
        } else if let Some(f) = l.strip_prefix("--- a/") {
            file = f.to_string();
        } else if let Some(rest) = l.strip_prefix("@@ -") {
            let old = rest.split(' ').next().unwrap_or("");
            let (a, b) = old.split_once(',').unwrap_or((old, "1"));
            if let (Ok(a), Ok(b)) = (a.parse::<usize>(), b.parse::<usize>()) {
                out.push(Hunk { file: file.clone(), old_start: a, old_len: b });
            }
        }
    }
    out
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Deviation {
    pub file: String,
    pub line: usize,
}

/// Wijzigingen buiten de toegestane bereiken. Regels in het bereik van een toegestane widget zijn goed; imports bovenin een
/// bestand waar iets mag veranderen tellen niet als afwijking (een nieuwe widget heeft soms een import nodig).
pub fn deviations(allowed: &[(String, usize, usize)], hunks: &[Hunk], new_files: &[String], file_src: &dyn Fn(&str) -> Option<String>) -> Vec<Deviation> {
    let mut out = vec![];
    for h in hunks {
        let ranges: Vec<&(String, usize, usize)> = allowed.iter().filter(|(f, _, _)| *f == h.file).collect();
        let inside = if h.old_len == 0 {
            ranges.iter().any(|(_, s, e)| h.old_start >= *s && h.old_start < *e)
        } else {
            let last = h.old_start + h.old_len - 1;
            ranges.iter().any(|(_, s, e)| h.old_start >= *s && last <= *e)
        };
        if inside {
            continue;
        }
        // Imports mogen, mits het bestand een toegestane widget bevat.
        if !ranges.is_empty() {
            if let Some(src) = file_src(&h.file) {
                let n = h.old_len.max(1);
                let all_imports = (h.old_start..h.old_start + n).all(|ln| src.lines().nth(ln.saturating_sub(1)).is_some_and(|l| l.trim_start().starts_with("import ")));
                if all_imports {
                    continue;
                }
            }
        }
        out.push(Deviation { file: h.file.clone(), line: h.old_start });
    }
    for f in new_files {
        out.push(Deviation { file: f.clone(), line: 0 });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIFF: &str = "diff --git a/lib/a.dart b/lib/a.dart\n--- a/lib/a.dart\n+++ b/lib/a.dart\n@@ -2,0 +3 @@ x\n+import 'z.dart';\n@@ -22,2 +23,3 @@ y\n-old\n-old2\n+new\n+new\n+new\ndiff --git a/lib/b.dart b/lib/b.dart\n--- a/lib/b.dart\n+++ b/lib/b.dart\n@@ -5 +5 @@\n-a\n+b\n";

    fn sel() -> Vec<(String, usize, usize)> {
        vec![("lib/a.dart".into(), 20, 30)]
    }

    #[test]
    fn hunks_are_parsed_with_old_ranges() {
        let h = parse_hunks(DIFF);
        assert_eq!(h.len(), 3);
        assert_eq!(h[0], Hunk { file: "lib/a.dart".into(), old_start: 2, old_len: 0 });
        assert_eq!(h[1], Hunk { file: "lib/a.dart".into(), old_start: 22, old_len: 2 });
        assert_eq!(h[2], Hunk { file: "lib/b.dart".into(), old_start: 5, old_len: 1 });
    }

    #[test]
    fn changes_inside_the_widget_pass_and_others_are_reported() {
        let src = |f: &str| (f == "lib/a.dart").then(|| "import 'x.dart';\nimport 'y.dart';\nclass A {}\n".to_string());
        let d = deviations(&sel(), &parse_hunks(DIFF), &[], &src);
        assert_eq!(d, vec![Deviation { file: "lib/b.dart".into(), line: 5 }], "the import is allowed, the other file is not");
        let d = deviations(&sel(), &parse_hunks(DIFF), &["lib/new.dart".into()], &src);
        assert_eq!(d.len(), 2);
    }

    #[test]
    fn partially_outside_and_insertions_at_the_edges() {
        let h = |s, l| vec![Hunk { file: "lib/a.dart".into(), old_start: s, old_len: l }];
        let none = |_: &str| None;
        assert!(deviations(&sel(), &h(29, 3), &[], &none).len() == 1, "lines 29–30 inside, 31 outside → the whole hunk is reported");
        assert!(deviations(&sel(), &h(30, 0), &[], &none).len() == 1, "an insertion after the closing line is outside");
        assert!(deviations(&sel(), &h(25, 0), &[], &none).is_empty(), "an insertion inside is fine");
        assert!(deviations(&sel(), &h(20, 11), &[], &none).is_empty());
    }

    #[test]
    fn the_prompt_states_facts_and_the_limit() {
        let s = Selection {
            screen: Some("HomeScreen".into()),
            screen_file: Some("lib/home_screen.dart".into()),
            targets: vec![Target { widget: "PrimaryButton".into(), text: Some("Open list".into()), file: "lib/home_screen.dart".into(), start_line: 22, end_line: 25, chain: vec![Link { widget: "PrimaryButton".into(), file: "lib/home_screen.dart".into(), line: 22, column: 13 }, Link { widget: "Column".into(), file: "lib/home_screen.dart".into(), line: 15, column: 16 }], via_shared: true }],
            bbox: Some([10.0, 20.0, 30.0, 8.0]),
        };
        let p = s.prompt();
        assert!(p.contains("PrimaryButton \"Open list\" — lib/home_screen.dart lines 22–25") && p.contains("shared component") && p.contains("inside Column (lib/home_screen.dart:15)") && p.contains("Change only code inside those line ranges"));
        assert_eq!(s.ranges(), vec![("lib/home_screen.dart".to_string(), 22, 25)]);
    }
}

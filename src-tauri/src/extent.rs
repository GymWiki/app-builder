//! Bereik van een widget-uitdrukking in Dart-code, vanaf de aanmaaklocatie die de widget inspector geeft
//! (bestand, regel, kolom van de constructor). We scannen tot de haakjes sluiten, met oog voor strings en
//! commentaar. Geen volledige parser: de plek waar de uitdrukking begint is al bekend, we zoeken alleen het einde.
//! (Afwijking van de opdracht, zie DECISIONS.md: geen tree-sitter-dart; een zware C-afhankelijkheid voor iets wat dit
//! kleine begin→eind-probleem oplost, en de scanner is getest met de gevallen uit echte projecten.)

/// Regelbereik, 1-gebaseerd en inclusief.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct Span {
    pub start_line: usize,
    pub end_line: usize,
}

/// `line` en `col` zijn 1-gebaseerd (zoals de inspector ze geeft); `col` wijst op de naam van de widget (of op `const`/`new`).
pub fn expression_span(src: &str, line: usize, col: usize) -> Option<Span> {
    let lines: Vec<&str> = src.split('\n').collect();
    if line == 0 || line > lines.len() {
        return None;
    }
    // Byte-offset van het begin van de uitdrukking.
    let mut offset = lines[..line - 1].iter().map(|l| l.len() + 1).sum::<usize>();
    let l = lines[line - 1];
    let col_bytes = l.char_indices().nth(col.saturating_sub(1)).map(|(i, _)| i).unwrap_or(0);
    offset += col_bytes;
    let b = src.as_bytes();
    let mut i = offset;
    // Voorloop: `const `/`new `.
    for kw in ["const ", "new "] {
        if src[i..].starts_with(kw) {
            i += kw.len();
        }
    }
    // Naam met punten (`ListView.builder`) en eventuele generics (`showDialog<void>`).
    let name_start = i;
    while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_' || b[i] == b'.' || b[i] == b'$') {
        i += 1;
    }
    if i == name_start {
        return None;
    }
    if i < b.len() && b[i] == b'<' {
        let mut depth = 0;
        while i < b.len() {
            match b[i] {
                b'<' => depth += 1,
                b'>' => {
                    depth -= 1;
                    if depth == 0 {
                        i += 1;
                        break;
                    }
                }
                _ => {}
            }
            i += 1;
        }
    }
    while i < b.len() && (b[i] == b' ' || b[i] == b'\n' || b[i] == b'\t' || b[i] == b'\r') {
        i += 1;
    }
    if i >= b.len() || b[i] != b'(' {
        return None;
    }
    let end = matching_paren(src, i)?;
    let end_line = src[..=end].matches('\n').count() + 1;
    Some(Span { start_line: line, end_line })
}

/// Index van het haakje dat bij het `(` op `open` hoort.
fn matching_paren(src: &str, open: usize) -> Option<usize> {
    let b = src.as_bytes();
    let mut depth = 0i32;
    let mut i = open;
    while i < b.len() {
        match b[i] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth -= 1;
                if depth == 0 {
                    return (b[i] == b')').then_some(i);
                }
            }
            b'/' if b.get(i + 1) == Some(&b'/') => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                i += 2;
                while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                    i += 1;
                }
                i += 2;
                continue;
            }
            q @ (b'\'' | b'"') => {
                i = skip_string(src, i, q, false)?;
                continue;
            }
            b'r' if matches!(b.get(i + 1), Some(b'\'') | Some(b'"')) && !prev_is_ident(b, i) => {
                let q = b[i + 1];
                i = skip_string(src, i + 1, q, true)?;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn prev_is_ident(b: &[u8], i: usize) -> bool {
    i > 0 && (b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_')
}

/// Geeft de index vlak ná het sluitende aanhalingsteken. Ondersteunt '''…''' / """…""", raw strings en `${…}`.
fn skip_string(src: &str, start: usize, q: u8, raw: bool) -> Option<usize> {
    let b = src.as_bytes();
    let triple = b.get(start + 1) == Some(&q) && b.get(start + 2) == Some(&q);
    let mut i = start + if triple { 3 } else { 1 };
    while i < b.len() {
        let c = b[i];
        if !raw && c == b'\\' {
            i += 2;
            continue;
        }
        if !raw && c == b'$' && b.get(i + 1) == Some(&b'{') {
            // Interpolatie: tot de bijpassende `}` (kan zelf strings bevatten).
            let mut depth = 0i32;
            let mut j = i + 1;
            while j < b.len() {
                match b[j] {
                    b'{' => depth += 1,
                    b'}' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    s @ (b'\'' | b'"') => {
                        j = skip_string(src, j, s, false)?;
                        continue;
                    }
                    _ => {}
                }
                j += 1;
            }
            i = j + 1;
            continue;
        }
        if c == q {
            if triple {
                if b.get(i + 1) == Some(&q) && b.get(i + 2) == Some(&q) {
                    return Some(i + 3);
                }
            } else {
                return Some(i + 1);
            }
        } else if c == b'\n' && !triple {
            return None; // niet-afgesloten string
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOME: &str = "import 'x.dart';\n\nclass A {\n  Widget build() {\n    return Column(\n      children: [\n        const Text('Welcome (back)', style: TextStyle(fontSize: 28)),\n        PrimaryButton(\n          label: 'Open list',\n          onPressed: () => Navigator.push(context, MaterialPageRoute(builder: (_) => const ListScreen())),\n        ),\n        OutlinedButton(\n          onPressed: () => showDialog<void>(\n            context: context,\n            builder: (ctx) => AlertDialog(\n              title: const Text('Hello'),\n            ),\n          ),\n          child: const Text('Open dialog'),\n        ),\n      ],\n    );\n  }\n}\n";

    fn at(line: usize, needle: &str) -> Option<Span> {
        let l = HOME.lines().nth(line - 1).unwrap();
        expression_span(HOME, line, l.find(needle).unwrap() + 1)
    }

    #[test]
    fn single_and_multi_line_widgets() {
        assert_eq!(at(7, "const Text"), Some(Span { start_line: 7, end_line: 7 }), "parentheses inside a string do not count");
        assert_eq!(at(8, "PrimaryButton"), Some(Span { start_line: 8, end_line: 11 }));
        assert_eq!(at(12, "OutlinedButton"), Some(Span { start_line: 12, end_line: 20 }));
        assert_eq!(at(5, "Column"), Some(Span { start_line: 5, end_line: 22 }));
    }

    #[test]
    fn generics_named_constructors_and_nesting() {
        assert_eq!(at(13, "showDialog"), Some(Span { start_line: 13, end_line: 18 }));
        let src = "x(\n  ListView.builder(\n    itemBuilder: (c, i) => Text('Item $i ${i + 1} \"q\"'),\n  ),\n);\n";
        assert_eq!(expression_span(src, 2, 3), Some(Span { start_line: 2, end_line: 4 }));
    }

    #[test]
    fn comments_raw_and_triple_quoted_strings() {
        let src = "a(\n  // closing ) in a comment\n  b: r'raw ) string',\n  c: '''multi\n) line''',\n  /* ) */\n);\n";
        assert_eq!(expression_span(src, 1, 1), Some(Span { start_line: 1, end_line: 7 }));
    }

    #[test]
    fn bad_input_gives_none() {
        assert_eq!(expression_span(HOME, 99, 1), None);
        assert_eq!(expression_span("Text('open", 1, 1), None);
        assert_eq!(expression_span("final x = 3;", 1, 7), None, "not a call");
    }
}

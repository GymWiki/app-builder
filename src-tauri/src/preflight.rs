//! Preflight-fixes: wat de checklist als ontbrekend meldt en wat Studio zelf kan oplossen (deterministisch, geen AI).
//! De checklist (`checklist.rs`) is de preflight; hier staat welke ontbrekende punten een "Los voor mij op" krijgen.

use crate::error::AppResult;
use std::path::Path;

/// (checklistpunt, ontbrekend onderdeel) → kan Studio zelf oplossen.
pub const FIXABLE: &[(&str, &str)] = &[
    ("app_basics", "version"),
    ("store_listing", "title"),
    ("store_listing", "short_description"),
    ("store_listing", "full_description"),
    ("version_changelog", "changelog"),
];

pub fn is_fixable(item: &str, missing: &str) -> bool {
    FIXABLE.iter().any(|(i, m)| *i == item && *m == missing)
}

/// Kort een tekst in op een woordgrens, met een maximum aantal tekens.
pub fn shorten(text: &str, max: usize) -> String {
    let t = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if t.chars().count() <= max {
        return t;
    }
    let cut: String = t.chars().take(max.saturating_sub(1)).collect();
    let cut = cut.rsplit_once(' ').map(|(a, _)| a.to_string()).filter(|a| !a.is_empty()).unwrap_or(cut);
    format!("{}…", cut.trim_end_matches(['.', ',', ';', ':', ' ']))
}

/// Zet `version: 1.0.0+1` in pubspec.yaml als er geen geldige versie staat.
pub fn fix_version(dir: &Path) -> AppResult<bool> {
    let p = dir.join("pubspec.yaml");
    let text = std::fs::read_to_string(&p)?;
    if crate::studio_tools::parse_pubspec_version(&text).is_some() {
        return Ok(false);
    }
    let mut out: Vec<String> = vec![];
    let mut done = false;
    for l in text.lines() {
        if l.starts_with("version:") {
            out.push("version: 1.0.0+1".into());
            done = true;
        } else {
            out.push(l.to_string());
        }
    }
    if !done {
        let at = out.iter().position(|l| l.starts_with("description:")).map(|i| i + 1).unwrap_or(out.len().min(1));
        out.insert(at, "version: 1.0.0+1".into());
    }
    let mut s = out.join("\n");
    s.push('\n');
    std::fs::write(p, s)?;
    Ok(true)
}

/// Vult lege winkelvelden uit de naam en beschrijving van het project (de gebruiker vult dat maar één keer in).
/// Geeft de velden terug die zijn ingevuld.
pub fn fix_listing(m: &mut crate::meta::Meta, name: &str, description: &str, default_email: &str) -> Vec<String> {
    let mut done = vec![];
    let desc = description.trim();
    if m.listing.title.is_empty() && !name.trim().is_empty() {
        m.listing.title = shorten(name, 30);
        done.push("title".into());
    }
    if m.listing.short_description.is_empty() && !desc.is_empty() {
        m.listing.short_description = shorten(desc, 80);
        done.push("short_description".into());
    }
    if m.listing.full_description.is_empty() && !desc.is_empty() {
        m.listing.full_description = shorten(desc, 4000);
        done.push("full_description".into());
    }
    if m.listing.contact_email.is_empty() && !default_email.trim().is_empty() {
        m.listing.contact_email = default_email.trim().to_string();
        done.push("contact_email".into());
    }
    done
}

/// Schrijft `CHANGELOG.md` uit de momentopnames sinds de laatste publicatie (of een eerste-versie-regel).
pub fn fix_changelog(dir: &Path, name: &str, version: &str, lines: &[String]) -> AppResult<bool> {
    let p = dir.join("CHANGELOG.md");
    if std::fs::read_to_string(&p).is_ok_and(|t| t.trim().len() > 20) {
        return Ok(false);
    }
    let body: Vec<String> = if lines.is_empty() { vec![format!("- First release of {name}")] } else { lines.iter().map(|l| format!("- {l}")).collect() };
    std::fs::write(p, format!("# Changelog\n\n## {version}\n\n{}\n", body.join("\n")))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_known_items_are_fixable() {
        assert!(is_fixable("app_basics", "version") && !is_fixable("app_basics", "icon") && !is_fixable("legal", "privacy_url"));
    }

    #[test]
    fn text_is_shortened_on_a_word_boundary() {
        assert_eq!(shorten("A very simple scoreboard for friday games", 20), "A very simple…");
        assert_eq!(shorten("short", 20), "short");
    }

    #[test]
    fn the_listing_is_filled_from_the_description_only_where_empty() {
        let mut m = crate::meta::Meta::default();
        m.listing.title = "Mine".into();
        let done = fix_listing(&mut m, "Friday Scores", "Keep score of the friday night games with your friends and see who wins the season.", "me@example.org");
        assert_eq!(m.listing.title, "Mine");
        assert_eq!(done, vec!["short_description", "full_description", "contact_email"]);
        assert_eq!(m.listing.contact_email, "me@example.org");
        assert!(m.listing.short_description.chars().count() <= 80);
        assert!(crate::meta::save(&std::env::temp_dir(), &m).is_ok());
        assert!(fix_listing(&mut m, "x", "y", "z@example.org").is_empty(), "nothing is filled twice");
    }

    #[test]
    fn version_and_changelog_are_written_once() {
        let d = std::env::temp_dir().join(format!("studio-preflight-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("pubspec.yaml"), "name: demo\ndescription: x\n").unwrap();
        assert!(fix_version(&d).unwrap());
        assert!(!fix_version(&d).unwrap());
        assert!(std::fs::read_to_string(d.join("pubspec.yaml")).unwrap().contains("version: 1.0.0+1"));
        assert!(fix_changelog(&d, "Demo", "1.0.0", &[]).unwrap());
        assert!(!fix_changelog(&d, "Demo", "1.0.0", &[]).unwrap());
        assert!(std::fs::read_to_string(d.join("CHANGELOG.md")).unwrap().contains("First release of Demo"));
    }
}

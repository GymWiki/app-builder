//! Afleidregels op één plek: wat uit één invoer volgt, vraagt Studio nooit opnieuw aan de gebruiker.
#![allow(dead_code)] // nog niet overal aangesloten (zie DECISIONS.md, Fase 13)

/// App-naam → deel van een bundle-ID: kleine letters/cijfers, geen accenten of spaties, begint niet met een cijfer.
pub fn id_part(name: &str) -> String {
    let lower = name.to_lowercase();
    let mut s: String = lower
        .chars()
        .map(|c| match c {
            'à' | 'á' | 'â' | 'ä' | 'ã' | 'å' => 'a',
            'è' | 'é' | 'ê' | 'ë' => 'e',
            'ì' | 'í' | 'î' | 'ï' => 'i',
            'ò' | 'ó' | 'ô' | 'ö' | 'õ' => 'o',
            'ù' | 'ú' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            'ñ' => 'n',
            c => c,
        })
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect();
    if s.is_empty() || s.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        s.insert_str(0, "app");
    }
    s
}

/// Voorstel voor een bundle-ID: `<omgekeerd domein of ontwikkelaar>.<app>`. Zonder ontwikkelaar: `com.example.<app>`
/// (de gebruiker ziet dat als voorstel dat hij kan overnemen; Studio vraagt de ontwikkelaar maar één keer).
pub fn bundle_id(developer: Option<&str>, app_name: &str) -> String {
    let dev = developer.map(id_part).filter(|_| developer.is_some_and(|x| !x.trim().is_empty())).unwrap_or_else(|| "example".into());
    format!("com.{dev}.{}", id_part(app_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_become_safe_id_parts() {
        assert_eq!(id_part("Friday Scores!"), "fridayscores");
        assert_eq!(id_part("Café Ünïcode"), "cafeunicode");
        assert_eq!(id_part("2048"), "app2048");
        assert_eq!(id_part("???"), "app");
    }

    #[test]
    fn a_bundle_id_is_proposed_from_the_name_and_developer() {
        assert_eq!(bundle_id(Some("Acme Studio"), "Friday Scores"), "com.acmestudio.fridayscores");
        assert_eq!(bundle_id(None, "Water Tracker"), "com.example.watertracker");
    }
}

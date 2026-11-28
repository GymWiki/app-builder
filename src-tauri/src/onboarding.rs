//! Onboarding van een nieuwe app (fase 14): functies voorstellen, stijlkaarten, het plan (blueprint) laten maken en een veilige terugval.
//! De AI wordt alleen gebruikt voor de lijst functies en het plan; stijl, palet en alles daarna is deterministisch.

use crate::blueprint::{self, Blueprint, Capabilities};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FeatureSuggestion {
    pub title: String,
    pub capability: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StyleCard {
    pub id: String,
    pub seed: String,
    pub font: String,
    pub kit: String,
}

/// Drie stijlvoorstellen; de volgorde hangt af van de beschrijving zodat verschillende apps niet steeds hetzelfde eerst krijgen.
pub fn style_cards(description: &str) -> Vec<StyleCard> {
    let mut cards = vec![
        StyleCard { id: "calm".into(), seed: "#0D9488".into(), font: "Inter".into(), kit: "rounded".into() },
        StyleCard { id: "lively".into(), seed: "#DB2777".into(), font: "Poppins".into(), kit: "rounded".into() },
        StyleCard { id: "sleek".into(), seed: "#2563EB".into(), font: "Roboto".into(), kit: "minimal".into() },
    ];
    let n = description.bytes().fold(0usize, |a, b| a.wrapping_add(b as usize)) % 3;
    cards.rotate_left(n);
    cards
}

/// Eerste JSON-array in een tekst.
pub fn extract_array(text: &str) -> Option<&str> {
    let start = text.find('[')?;
    let end = text.rfind(']')?;
    (end > start).then(|| &text[start..=end])
}

pub fn features_prompt(description: &str, caps: &Capabilities, language: &str) -> String {
    let ids = caps.supported.iter().chain(&caps.optional).map(|c| c.id.as_str()).collect::<Vec<_>>().join(", ");
    format!(
        "Suggest 4 to 8 features for this mobile app. Answer with ONLY a JSON array of objects {{\"title\": short plain-language feature name in the language \"{language}\", \"capability\": one of: {ids}}}. \
         Prefer features that work without an account and without internet. No questions, no explanation.\nApp description: {description}"
    )
}

/// Leest de lijst van de AI; alleen bekende functies komen erdoor (de rest is niet te bouwen binnen de catalogus).
pub fn parse_features(text: &str, caps: &Capabilities) -> Vec<FeatureSuggestion> {
    let Some(arr) = extract_array(text) else { return vec![] };
    let list: Vec<FeatureSuggestion> = serde_json::from_str(arr).unwrap_or_default();
    let mut seen = std::collections::BTreeSet::new();
    list.into_iter().filter(|f| caps.is_known(&f.capability) && !f.title.trim().is_empty() && f.title.chars().count() <= 80 && seen.insert(f.title.clone())).take(8).collect()
}

/// Standaardfuncties als de AI niet antwoordt: de flow blijft werken (de gebruiker kan ze aanpassen).
pub fn default_features(language: &str) -> Vec<FeatureSuggestion> {
    let nl = language.starts_with("nl");
    let t = |en: &str, nl_: &str| if nl { nl_ } else { en }.to_string();
    vec![
        FeatureSuggestion { title: t("See everything in a list", "Alles in een lijst zien"), capability: "lists".into() },
        FeatureSuggestion { title: t("Add and change items", "Dingen toevoegen en aanpassen"), capability: "forms".into() },
        FeatureSuggestion { title: t("Find things quickly", "Snel iets terugvinden"), capability: "search_filter".into() },
        FeatureSuggestion { title: t("Settings", "Instellingen"), capability: "settings".into() },
    ]
}

fn short_name(description: &str, language: &str) -> String {
    let words: Vec<String> = description
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() > 4)
        .take(2)
        .map(|w| {
            let mut c = w.chars();
            c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
        })
        .collect();
    let name = words.join(" ");
    if name.is_empty() { if language.starts_with("nl") { "Mijn app".into() } else { "My app".into() } } else { name.chars().take(30).collect() }
}

/// Een kleine maar complete blueprint als de AI geen plan levert: Studio blijft bruikbaar (en zegt dat ook in de aannames).
pub fn fallback_blueprint(description: &str, features: &[FeatureSuggestion], card: &StyleCard, language: &str) -> Blueprint {
    use blueprint::*;
    let nl = language.starts_with("nl");
    let t = |en: &str, nl_: &str| if nl { nl_ } else { en }.to_string();
    let feats: Vec<Feature> = features.iter().enumerate().map(|(i, f)| Feature { id: format!("f{}", i + 1), title: f.title.clone(), capability: f.capability.clone(), pages: vec!["home".into()], status: "now".into() }).filter(|f| capabilities().supported.iter().any(|c| c.id == f.capability)).collect();
    Blueprint {
        version: 1,
        app: AppInfo { name: short_name(description, language), purpose: description.chars().take(300).collect(), audience: String::new(), language: language.chars().take(2).collect(), description: description.chars().take(4000).collect() },
        style: Style { card: card.id.clone(), seed: card.seed.clone(), font: card.font.clone(), kit: card.kit.clone(), logo: None },
        navigation: Navigation { kind: "tabs".into(), order: vec!["home".into(), "settings".into()] },
        pages: vec![
            Page { id: "home".into(), name: t("Home", "Start"), purpose: t("The main page of the app", "De hoofdpagina van de app"), route: "/".into(), components: vec![t("main content", "hoofdinhoud")], models: vec![], features: feats.iter().map(|f| f.id.clone()).collect(), links: vec![] },
            Page { id: "settings".into(), name: t("Settings", "Instellingen"), purpose: t("Name and theme", "Naam en thema"), route: "/settings".into(), components: vec![t("theme choice", "themakeuze")], models: vec![], features: vec![], links: vec![] },
        ],
        models: vec![],
        features: feats,
        integrations: Integrations::default(),
        assumptions: vec![Assumption { id: "fallback".into(), text: t("I could not make a detailed plan, so this is a simple start. You can add pages and features.", "Ik kon geen uitgebreid plan maken, dus dit is een eenvoudige start. Je kunt pagina’s en functies toevoegen."), editable: true }, Assumption { id: "local".into(), text: t("Everything is stored on the phone, no account.", "Alles wordt op de telefoon bewaard, geen account."), editable: true }],
    }
}

/// Past de gekozen stijl toe op een blueprint (deterministisch; palet en tokens volgen daaruit via de configuratiemotor).
pub fn apply_style(bp: &mut Blueprint, card: &StyleCard, seed_override: Option<&str>) {
    bp.style.card = card.id.clone();
    bp.style.seed = seed_override.filter(|s| crate::studio_tools::valid_hex(s)).unwrap_or(&card.seed).to_uppercase();
    bp.style.font = card.font.clone();
    bp.style.kit = card.kit.clone();
}

/// Vraagt het plan en geeft het terug met `fallback = true` als de AI niets bruikbaars leverde.
pub fn make_plan(ask: &mut dyn FnMut(&str) -> Option<String>, description: &str, features: &[String], card: &StyleCard, language: &str) -> (Blueprint, bool, Vec<String>) {
    let caps = blueprint::capabilities();
    let hint = format!("{} look: main color {}, font {}, kit {}", card.id, card.seed, card.font, card.kit);
    let prompt = blueprint::plan_prompt(description, features, Some(&hint), language);
    match blueprint::plan(ask, &prompt, &caps) {
        Ok(mut bp) => {
            apply_style(&mut bp, card, None);
            (bp, false, vec![])
        }
        Err(blueprint::PlanError::Invalid(bp, issues)) => {
            // Het plan klopt niet helemaal: de gebruiker ziet het en de problemen; scaffold weigert het pas bij "Bouw mijn app".
            let mut bp = *bp;
            apply_style(&mut bp, card, None);
            (bp, false, issues)
        }
        Err(blueprint::PlanError::NoAnswer) => (fallback_blueprint(description, &default_features(language), card, language), true, vec![]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_style_cards_in_a_stable_description_dependent_order() {
        let a = style_cards("scorebord");
        assert_eq!(a.len(), 3);
        assert_eq!(a, style_cards("scorebord"));
        let ids: std::collections::BTreeSet<_> = a.iter().map(|c| c.id.clone()).collect();
        assert_eq!(ids.len(), 3);
        assert!(a.iter().all(|c| crate::studio_tools::valid_hex(&c.seed) && crate::config::FONTS.contains(&c.font.as_str()) && crate::config::KITS.contains(&c.kit.as_str())));
    }

    #[test]
    fn only_known_features_survive_the_ai_list() {
        let caps = blueprint::capabilities();
        let text = "Here you go:\n[{\"title\":\"Players\",\"capability\":\"lists\"},{\"title\":\"Live play\",\"capability\":\"realtime_multiplayer\"},{\"title\":\"Players\",\"capability\":\"forms\"},{\"title\":\"\",\"capability\":\"lists\"}]";
        let f = parse_features(text, &caps);
        assert_eq!(f, vec![FeatureSuggestion { title: "Players".into(), capability: "lists".into() }]);
        assert!(parse_features("sorry", &caps).is_empty());
        assert!(default_features("nl")[0].title.contains("lijst"));
    }

    #[test]
    fn the_fallback_blueprint_is_always_valid_and_says_so() {
        for desc in ["", "Een scorebord voor onze vrijdagavond-spelletjes", "x"] {
            for lang in ["nl", "en"] {
                let card = &style_cards(desc)[0];
                let bp = fallback_blueprint(desc, &default_features(lang), card, lang);
                assert_eq!(blueprint::validate(&bp, &blueprint::capabilities()), Vec::<String>::new(), "{desc}/{lang}");
                assert!(bp.assumptions.iter().any(|a| a.id == "fallback"));
            }
        }
        assert_eq!(fallback_blueprint("Een scorebord voor vrienden", &[], &style_cards("")[0], "nl").app.name, "Scorebord Vrienden");
    }

    #[test]
    fn a_plan_without_an_answer_falls_back_and_a_good_one_gets_the_chosen_style() {
        let card = style_cards("x")[1].clone();
        let (bp, fallback, issues) = make_plan(&mut |_| None, "scoreboard", &[], &card, "en");
        assert!(fallback && issues.is_empty() && bp.style.seed == card.seed);
        let good = serde_json::to_string(&blueprint::sample()).unwrap();
        let (bp, fallback, issues) = make_plan(&mut |_| Some(good.clone()), "scoreboard", &[], &card, "en");
        assert!(!fallback && issues.is_empty());
        assert_eq!((bp.style.seed.as_str(), bp.style.font.as_str(), bp.style.kit.as_str()), (card.seed.as_str(), card.font.as_str(), card.kit.as_str()));
        let mut bad = blueprint::sample();
        bad.pages[0].route = "x".into();
        let bad = serde_json::to_string(&bad).unwrap();
        let (_, fallback, issues) = make_plan(&mut |_| Some(bad.clone()), "scoreboard", &[], &card, "en");
        assert!(!fallback && !issues.is_empty(), "a plan with problems is shown to the user, with the problems");
    }
}

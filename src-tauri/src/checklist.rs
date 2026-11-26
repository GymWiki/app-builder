//! De checklist-poort: "Publiceren" blijft uitgeschakeld tot alle verplichte punten groen zijn.
//! Zuivere logica: `evaluate` krijgt feiten en geeft per punt terug wat er mist en wat de volgende
//! actie is (de UI vertaalt de sleutels). Verplichte en advies-punten zijn strikt gescheiden.

use crate::meta::Meta;
use serde::Serialize;

#[derive(Debug, Clone, Default)]
pub struct Facts {
    pub bundle_id: String,
    pub version: String,
    pub has_icon: bool,
    pub screenshots: usize,
    pub privacy_docs: bool,
    pub changelog: bool,
    pub signing_configured: bool,
    /// Vingerafdruk waarop de testrun voor het laatst slaagde (None = nooit geslaagd).
    pub test_passed_fingerprint: Option<String>,
    /// Is er na een geslaagde run een mislukte geweest, of is er nog nooit een run geweest?
    pub test_ever_ran: bool,
    pub test_failed_last: bool,
    pub current_fingerprint: String,
    /// Native functies die in de code zijn herkend (camera, push, …); die moeten handmatig getest.
    pub native_features: Vec<String>,
    /// Verplichte geheimen die nog ontbreken (uit de kluis); per checklistpunt een knop naar het juiste kopje.
    pub missing_secrets: Vec<crate::vault::MissingSecret>,
    /// Aantal geheimen dat in de projectbestanden is gevonden (lekcontrole).
    pub leaks: usize,
}

/// Ontbrekende geheimen van een checklistpunt als `secret:<integratie>.<veld>`; de eerste bepaalt waar de knop naartoe gaat.
fn secret_items(f: &Facts, key: &str, miss: &mut Vec<String>) -> Option<String> {
    let mut next = None;
    for m in f.missing_secrets.iter().filter(|m| m.items.iter().any(|i| i == key)) {
        miss.push(if m.field.is_empty() { format!("secret_problem:{}", m.integration) } else { format!("secret:{}.{}", m.integration, m.field) });
        next.get_or_insert_with(|| format!("{}:{}", if m.enabled { "secrets" } else { "integrations" }, m.integration));
    }
    next
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ItemState {
    pub key: String,
    pub blocking: bool,
    /// done | todo | failed
    pub status: String,
    /// Wat er nog mist (sleutels voor de UI).
    pub missing: Vec<String>,
    /// Volgende actie: naam van een sectie of tab ("settings", "assets", "data", "publish:legal", …).
    pub next: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Evaluation {
    pub items: Vec<ItemState>,
    pub can_publish: bool,
    pub done: usize,
    pub total: usize,
}

fn item(key: &str, blocking: bool, missing: Vec<&str>, next: &str) -> ItemState {
    let done = missing.is_empty();
    ItemState {
        key: key.into(),
        blocking,
        status: if done { "done" } else { "todo" }.into(),
        missing: missing.into_iter().map(String::from).collect(),
        next: (!done).then(|| next.to_string()),
    }
}

pub fn evaluate(m: &Meta, f: &Facts) -> Evaluation {
    let mut items = vec![];

    // 1. app-basis
    let mut miss = vec![];
    if f.bundle_id.is_empty() || f.bundle_id.starts_with("com.example.") {
        miss.push("bundle_id");
    }
    if !f.has_icon {
        miss.push("icon");
    }
    if f.version.is_empty() {
        miss.push("version");
    }
    let next = if miss.contains(&"icon") { "assets" } else { "settings" };
    items.push(item("app_basics", true, miss, next));

    // 2. data en login (bij accounts: account verwijderen is verplicht in beide winkels)
    let mut miss = vec![];
    if m.data.provider.is_empty() {
        miss.push("provider");
    }
    let has_accounts = m.questionnaire.has_accounts == Some(true) || !m.data.auth_methods.is_empty();
    if has_accounts && !m.legal.account_deletion {
        miss.push("account_deletion");
    }
    let mut miss: Vec<String> = miss.into_iter().map(String::from).collect();
    let secret_next = secret_items(f, "data_auth", &mut miss);
    let mut it = item("data_auth", true, vec![], "data");
    it.status = if miss.is_empty() { "done" } else { "todo" }.into();
    it.next = (!miss.is_empty()).then(|| if miss.iter().all(|m| m.starts_with("secret:")) { secret_next.clone().unwrap_or_else(|| "data".into()) } else { "data".into() });
    it.missing = miss;
    items.push(it);

    // 3. juridisch
    let mut miss = vec![];
    if !f.privacy_docs {
        miss.push("docs");
    }
    if m.legal.privacy_url.is_empty() {
        miss.push("privacy_url");
    }
    if m.legal.terms_url.is_empty() {
        miss.push("terms_url");
    }
    if !m.legal.privacy_url.is_empty() && !m.legal.hosted_checked {
        miss.push("hosted_checked");
    }
    items.push(item("legal", true, miss, "publish:legal"));

    // 4. AI-testrun op de huidige versie van de code
    let mut t = item("ai_test_run", true, vec![], "publish:test");
    match &f.test_passed_fingerprint {
        Some(fp) if *fp == f.current_fingerprint => {}
        Some(_) => {
            t.status = "todo".into();
            t.missing = vec!["expired".into()];
        }
        None if f.test_failed_last => {
            t.status = "failed".into();
            t.missing = vec!["failed".into()];
        }
        None => {
            t.status = "todo".into();
            t.missing = vec![if f.test_ever_ran { "failed" } else { "never" }.into()];
        }
    }
    t.next = (t.status != "done").then(|| "publish:test".to_string());
    items.push(t);

    // 5. winkelvermelding
    let l = &m.listing;
    let mut miss = vec![];
    for (v, k) in [(l.title.is_empty(), "title"), (l.short_description.is_empty(), "short_description"), (l.full_description.is_empty(), "full_description"), (l.category.is_empty(), "category"), (l.contact_email.is_empty(), "contact_email")] {
        if v {
            miss.push(k);
        }
    }
    if f.screenshots < 2 {
        miss.push("screenshots");
    }
    if !m.questionnaire.all_answered() {
        miss.push("questionnaire");
    } else if !m.questionnaire.play_console_done {
        miss.push("play_console");
    }
    items.push(item("store_listing", true, miss, "publish:listing"));

    // 6. accounts en signing (Android eerst). De geheimen komen uit de kluis; er wordt nooit iets dubbel gevraagd.
    let mut miss: Vec<String> = vec![];
    let secret_next = secret_items(f, "accounts_signing", &mut miss);
    let has_secret_gap = !miss.is_empty();
    if !f.signing_configured {
        miss.push("gradle".into());
    }
    if !m.publishing.play_account_ready {
        miss.push("play_account".into());
    }
    let mut it = item("accounts_signing", true, vec![], "publish:signing");
    it.status = if miss.is_empty() { "done" } else { "todo" }.into();
    it.next = (!miss.is_empty()).then(|| if has_secret_gap { secret_next.clone().unwrap_or_else(|| "publish:signing".into()) } else { "publish:signing".into() });
    it.missing = miss;
    items.push(it);

    // 7. versie en changelog
    let mut miss = vec![];
    // Een al gepubliceerde versie blokkeert niet meer: Studio verhoogt versie en buildnummer zelf vlak voor de upload.
    if !f.changelog {
        miss.push("changelog");
    }
    let next = if miss.contains(&"changelog") { "versions" } else { "settings" };
    items.push(item("version_changelog", true, miss, next));

    // 7b. geen geheimen in de projectbestanden
    let mut it = item("no_leaks", true, vec![], "leaks");
    if f.leaks > 0 {
        it.status = "failed".into();
        it.missing = vec![format!("leaks:{}", f.leaks)];
        it.next = Some("leaks".into());
    }
    items.push(it);

    // 8. laatste bevestiging: pas geldig als al het andere groen is
    let others_done = items.iter().all(|i| i.status == "done");
    let mut fin = item("final_confirmation", true, vec![], "publish:confirm");
    if !others_done {
        fin.status = "todo".into();
        fin.missing = vec!["others".into()];
        fin.next = items.iter().find(|i| i.status != "done").and_then(|i| i.next.clone());
    } else if !m.publishing.final_confirmed {
        fin.status = "todo".into();
        fin.missing = vec!["confirm".into()];
        fin.next = Some("publish:confirm".into());
    } else {
        fin.next = None;
    }
    items.push(fin);

    // adviespunten
    items.push(item("device_test", false, if m.publishing.device_tested { vec![] } else { vec!["device"] }, "preview"));
    let unchecked: Vec<&str> = f.native_features.iter().filter(|k| !m.publishing.manual_checks.get(*k).copied().unwrap_or(false)).map(String::as_str).collect();
    items.push(item("native_manual", false, unchecked, "publish:test"));

    let blocking: Vec<&ItemState> = items.iter().filter(|i| i.blocking).collect();
    let done = blocking.iter().filter(|i| i.status == "done").count();
    Evaluation { can_publish: done == blocking.len(), done, total: blocking.len(), items: items.clone() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ready() -> (Meta, Facts) {
        let mut m = Meta::default();
        m.data.provider = "local".into();
        m.legal.privacy_url = "https://example.com/p".into();
        m.legal.terms_url = "https://example.com/t".into();
        m.legal.hosted_checked = true;
        m.listing.title = "Scores".into();
        m.listing.short_description = "Keep score".into();
        m.listing.full_description = "A scoreboard".into();
        m.listing.category = "tools".into();
        m.listing.contact_email = "me@example.com".into();
        m.questionnaire = crate::meta::Questionnaire {
            has_accounts: Some(false),
            collects_personal_data: Some(false),
            shows_ads: Some(false),
            in_app_purchases: Some(false),
            user_generated_content: Some(false),
            uses_location: Some(false),
            play_console_done: true,
        };
        m.publishing.play_account_ready = true;
        m.publishing.final_confirmed = true;
        let f = Facts {
            bundle_id: "nl.bakker.scores".into(),
            version: "1.1.0".into(),
            has_icon: true,
            screenshots: 3,
            privacy_docs: true,
            changelog: true,
            signing_configured: true,
            test_passed_fingerprint: Some("abc".into()),
            current_fingerprint: "abc".into(),
            test_ever_ran: true,
            ..Default::default()
        };
        (m, f)
    }

    fn ms(integration: &str, field: &str, enabled: bool) -> crate::vault::MissingSecret {
        crate::vault::MissingSecret { integration: integration.into(), field: field.into(), enabled, items: vec!["accounts_signing".into()] }
    }

    fn get<'a>(e: &'a Evaluation, k: &str) -> &'a ItemState {
        e.items.iter().find(|i| i.key == k).unwrap()
    }

    #[test]
    fn a_fresh_project_is_blocked_and_every_item_says_what_is_missing() {
        let e = evaluate(&Meta::default(), &Facts { version: "1.0.0".into(), bundle_id: "com.example.demo".into(), ..Default::default() });
        assert!(!e.can_publish);
        assert_eq!((e.done, e.total), (1, 9), "only the leak check is green on an empty project");
        for i in e.items.iter().filter(|i| i.blocking && i.status != "done") {
            assert!(!i.missing.is_empty() && i.next.is_some(), "{} must explain what is missing", i.key);
        }
        assert_eq!(get(&e, "app_basics").missing, ["bundle_id", "icon"]);
        assert_eq!(get(&e, "app_basics").next.as_deref(), Some("assets"));
        assert_eq!(get(&e, "final_confirmation").missing, ["others"]);
    }

    #[test]
    fn everything_green_opens_the_gate() {
        let (m, f) = ready();
        let e = evaluate(&m, &f);
        assert!(e.can_publish, "{:?}", e.items.iter().filter(|i| i.status != "done").collect::<Vec<_>>());
        assert_eq!((e.done, e.total), (9, 9));
    }

    #[test]
    fn any_single_missing_blocking_item_closes_the_gate() {
        type Tweak = Box<dyn Fn(&mut Meta, &mut Facts)>;
        let tweaks: Vec<(&str, Tweak)> = vec![
            ("app_basics", Box::new(|_, f| f.has_icon = false)),
            ("data_auth", Box::new(|m, _| m.data.provider.clear())),
            ("legal", Box::new(|m, _| m.legal.terms_url.clear())),
            ("ai_test_run", Box::new(|_, f| f.test_passed_fingerprint = None)),
            ("store_listing", Box::new(|_, f| f.screenshots = 1)),
            ("accounts_signing", Box::new(|_, f| f.missing_secrets = vec![ms("google_play", "keystore", true)])),
            ("no_leaks", Box::new(|_, f| f.leaks = 2)),
            ("version_changelog", Box::new(|_, f| f.changelog = false)),
        ];
        for (key, tweak) in tweaks {
            let (mut m, mut f) = ready();
            tweak(&mut m, &mut f);
            let e = evaluate(&m, &f);
            assert!(!e.can_publish, "{key} should block");
            assert!(get(&e, key).status != "done", "{key}");
            // de laatste bevestiging is dan ook niet geldig
            assert_eq!(get(&e, "final_confirmation").missing, ["others"], "{key}");
        }
    }

    #[test]
    fn test_run_expires_when_the_code_changes() {
        let (m, mut f) = ready();
        f.current_fingerprint = "changed".into();
        let e = evaluate(&m, &f);
        assert_eq!(get(&e, "ai_test_run").missing, ["expired"]);
        assert!(!e.can_publish);
    }

    #[test]
    fn distinguishes_never_ran_from_failed() {
        let (m, mut f) = ready();
        f.test_passed_fingerprint = None;
        f.test_ever_ran = false;
        assert_eq!(get(&evaluate(&m, &f), "ai_test_run").missing, ["never"]);
        f.test_ever_ran = true;
        f.test_failed_last = true;
        let e = evaluate(&m, &f);
        assert_eq!((get(&e, "ai_test_run").status.as_str(), get(&e, "ai_test_run").missing.clone()), ("failed", vec!["failed".to_string()]));
    }

    #[test]
    fn accounts_require_account_deletion() {
        let (mut m, f) = ready();
        m.questionnaire.has_accounts = Some(true);
        let e = evaluate(&m, &f);
        assert_eq!(get(&e, "data_auth").missing, ["account_deletion"]);
        m.legal.account_deletion = true;
        assert!(evaluate(&m, &f).can_publish);
        // ook via gekozen inlogmethodes
        let (mut m, f) = ready();
        m.data.auth_methods = vec!["email".into()];
        assert_eq!(get(&evaluate(&m, &f), "data_auth").missing, ["account_deletion"]);
    }

    #[test]
    fn republishing_the_same_version_does_not_block_because_studio_bumps_it() {
        let (mut m, f) = ready();
        m.last_published_version = Some("1.1.0".into());
        assert!(get(&evaluate(&m, &f), "version_changelog").missing.is_empty());
    }

    #[test]
    fn advisory_items_never_block() {
        let (mut m, mut f) = ready();
        f.native_features = vec!["camera".into(), "push".into()];
        let e = evaluate(&m, &f);
        assert!(e.can_publish, "advice must not block");
        assert_eq!(get(&e, "native_manual").missing, ["camera", "push"]);
        assert!(!get(&e, "native_manual").blocking && !get(&e, "device_test").blocking);
        m.publishing.manual_checks.insert("camera".into(), true);
        assert_eq!(get(&evaluate(&m, &f), "native_manual").missing, ["push"]);
        assert_eq!(evaluate(&m, &f).total, 9, "advisory items are not counted in X/9");
    }

    #[test]
    fn final_confirmation_needs_an_explicit_tick() {
        let (mut m, f) = ready();
        m.publishing.final_confirmed = false;
        let e = evaluate(&m, &f);
        assert_eq!(get(&e, "final_confirmation").missing, ["confirm"]);
        assert!(!e.can_publish);
    }

    #[test]
    fn unanswered_questionnaire_and_console_confirmation_are_distinct() {
        let (mut m, f) = ready();
        m.questionnaire.play_console_done = false;
        assert_eq!(get(&evaluate(&m, &f), "store_listing").missing, ["play_console"]);
        m.questionnaire.uses_location = None;
        assert_eq!(get(&evaluate(&m, &f), "store_listing").missing, ["questionnaire"]);
    }

    #[test]
    fn missing_secrets_point_to_the_right_heading_and_are_never_asked_twice() {
        let (m, mut f) = ready();
        f.missing_secrets = vec![ms("google_play", "keystore", true), ms("google_play", "keystore_password", true), ms("google_play", "service_account", false)];
        let e = evaluate(&m, &f);
        let it = get(&e, "accounts_signing");
        assert_eq!(it.missing, ["secret:google_play.keystore", "secret:google_play.keystore_password", "secret:google_play.service_account"]);
        assert_eq!(it.next.as_deref(), Some("secrets:google_play"), "the first gap decides where the button goes");
        assert!(!it.missing.iter().any(|x| x == "keystore"), "the old duplicate key is gone");
        // Staat de integratie uit, dan wijst de knop naar het integratie-overzicht.
        f.missing_secrets = vec![ms("google_play", "service_account", false)];
        assert_eq!(get(&evaluate(&m, &f), "accounts_signing").next.as_deref(), Some("integrations:google_play"));
        // Andere punten blijven onaangetast.
        assert_eq!(get(&evaluate(&m, &f), "data_auth").status, "done");
    }

    #[test]
    fn secrets_for_the_data_integration_land_on_the_data_item() {
        let (m, mut f) = ready();
        f.missing_secrets = vec![crate::vault::MissingSecret { integration: "supabase".into(), field: "url".into(), enabled: true, items: vec!["data_auth".into()] }];
        let e = evaluate(&m, &f);
        assert_eq!(get(&e, "data_auth").missing, ["secret:supabase.url"]);
        assert_eq!(get(&e, "data_auth").next.as_deref(), Some("secrets:supabase"));
        assert!(!e.can_publish);
        // Ontbreekt ook iets anders, dan gaat de knop naar de Data-pagina.
        let (mut m2, f2) = (m, f);
        m2.data.provider.clear();
        assert_eq!(get(&evaluate(&m2, &f2), "data_auth").next.as_deref(), Some("data"));
    }

    #[test]
    fn a_leak_blocks_publishing_and_says_how_many() {
        let (m, mut f) = ready();
        f.leaks = 3;
        let e = evaluate(&m, &f);
        assert!(!e.can_publish);
        let it = get(&e, "no_leaks");
        assert_eq!((it.status.as_str(), it.missing.clone(), it.next.as_deref()), ("failed", vec!["leaks:3".to_string()], Some("leaks")));
        assert!(it.blocking);
    }

    #[test]
    fn an_expired_or_invalid_key_shows_up_in_the_checklist_as_a_problem() {
        let (m, mut f) = ready();
        f.missing_secrets = vec![ms("apple", "", true)];
        let e = evaluate(&m, &f);
        let it = get(&e, "accounts_signing");
        assert_eq!(it.missing, ["secret_problem:apple"]);
        assert_eq!(it.next.as_deref(), Some("secrets:apple"));
        assert!(!e.can_publish);
    }
}

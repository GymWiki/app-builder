//! De opdrachten die "Add to app" / "Update app" aan Claude geeft (code), het voorstel voor het gating-plan, en de begeleide
//! stappen voor de winkels. Dit bestand bevat alleen tekst en logica zonder neveneffecten, zodat het goed te testen is.

use crate::purchases::*;

/// Mappen waar alleen de flow van de Aankopen-pagina in mag schrijven (losse chatopdrachten niet).
pub const PROTECTED_PREFIXES: &[&str] = &["lib/core/purchases/", "lib/features/paywall/"];

pub fn is_protected(rel: &str) -> bool {
    let rel = rel.trim_start_matches("./");
    PROTECTED_PREFIXES.iter().any(|p| rel.starts_with(p))
}

fn kind_word(k: Kind) -> &'static str {
    match k {
        Kind::OneTime => "one-time purchase (non-consumable)",
        Kind::Consumable => "consumable",
        Kind::Subscription => "auto-renewing subscription",
    }
}

fn period_word(p: Option<Period>) -> &'static str {
    match p {
        Some(Period::Week) => "week",
        Some(Period::Month) => "month",
        Some(Period::Year) => "year",
        None => "",
    }
}

fn product_line(bundle: &str, p: &Product) -> String {
    let mut s = format!("- id \"{}\" ({}), name \"{}\", gives \"{}\"", full_id(bundle, &p.id), kind_word(p.kind), p.name, if p.entitlement.is_empty() { "nothing (consumable)" } else { &p.entitlement });
    if let Some(per) = p.period {
        s.push_str(&format!(", renews every {}", period_word(Some(per))));
    }
    if let Some(t) = p.trial_days {
        s.push_str(&format!(", {t}-day free trial"));
    }
    s
}

/// De opdracht voor Claude: alleen de vaste, kleine structuur, en alleen het verschil bij "Update app".
pub fn codegen_prompt(p: &Purchases, plan: &ApplyPlan, bundle: &str, privacy_url: &str, terms_url: &str) -> String {
    let provider = match p.provider {
        Provider::RevenueCat => "RevenueCat (package `purchases_flutter`); read the public SDK key from `String.fromEnvironment('REVENUECAT_SDK_KEY')`. Never write a key into the code.",
        Provider::InAppPurchase => "the official `in_app_purchase` package.",
    };
    let mut s = String::from("STUDIO PURCHASES FLOW. This is the only way the purchases code may be changed. Work only in lib/core/purchases/, lib/features/paywall/, the gated places listed below, pubspec.yaml and test/.\n\n");
    if plan.first_time {
        s.push_str("This is the first time: create the whole structure.\n");
    } else {
        s.push_str("This is an UPDATE. Process only the difference listed under 'Changes'. Leave everything else exactly as it is.\n");
    }
    s.push_str(&format!("\nProvider: {provider}\n\nProducts (product ids are permanent in the stores; use them exactly):\n"));
    for pr in &p.products {
        s.push_str(&product_line(bundle, pr));
        s.push('\n');
    }
    s.push_str("\nChanges:\n");
    let list = |label: &str, v: &[String]| if v.is_empty() { String::new() } else { format!("- {label}: {}\n", v.join(", ")) };
    s.push_str(&list("new products", &plan.create));
    s.push_str(&list("changed products", &plan.update));
    s.push_str(&list("removed products (remove from the paywall and the provider; keep entitlements working for existing buyers)", &plan.remove));
    if plan.gate_points_added + plan.gate_points_removed > 0 {
        s.push_str(&format!("- gating: {} place(s) to lock, {} to unlock again\n", plan.gate_points_added, plan.gate_points_removed));
    }
    if plan.paywall_changed {
        s.push_str("- the paywall changed (see below)\n");
    }
    if plan.provider_changed {
        s.push_str("- the provider changed: replace the provider implementation, keep the PurchaseProvider interface\n");
    }
    s.push_str("\nStructure (keep it small):\n");
    s.push_str("1. lib/core/purchases/purchase_provider.dart: `abstract class PurchaseProvider` with loadProducts() (returns store products with the LOCAL price string and currency from the store), buy(productId), restore(), and a stream of active entitlements.\n");
    s.push_str("2. One implementation for the chosen provider, and `fake_purchase_provider.dart`: a FakePurchaseProvider that never contacts a store, returns fake products with fake prices, and 'buys' instantly. Use it when `const bool.fromEnvironment('STUDIO_FAKE_PURCHASES')` is true; Studio's test run sets that flag, so the test run can never make a real purchase.\n");
    s.push_str("3. lib/core/purchases/entitlements.dart: `Entitlements` with `has('pro')`, storage and restore.\n");
    s.push_str("4. A reusable `ProGate` widget (and a function) in lib/core/purchases/: shows its child when the entitlement is active, otherwise a clearly locked state that opens the paywall.\n");
    s.push_str("5. lib/features/paywall/paywall_screen.dart in the style of the app: use ONLY theme tokens and the app's UI-kit components (no hardcoded colours or fonts).\n\n");
    s.push_str("Paywall rules (must all hold):\n- The price shown always comes from the store product (local price and currency). NEVER hardcode an amount. While loading show a loading state; on an error show a calm message with a retry button; never a made-up price.\n- Show the price AND the period for subscriptions, and the free-trial length when there is one.\n- A 'Restore purchases' button.\n- Links to the terms and the privacy policy");
    if !terms_url.is_empty() || !privacy_url.is_empty() {
        s.push_str(&format!(" (terms: {}, privacy: {})", if terms_url.is_empty() { "<ask the user>" } else { terms_url }, if privacy_url.is_empty() { "<ask the user>" } else { privacy_url }));
    }
    s.push_str(".\n- A clear close button.\n");
    let w = &p.paywall;
    s.push_str(&format!("\nPaywall settings: appears {}; layout {}; headline \"{}\"; text \"{}\"; button \"{}\"; products in this order: {}; featured: {}.\n",
        match w.placement { Placement::Startup => "at startup/onboarding".to_string(), Placement::LockedFeature => "when a locked feature is tapped".to_string(), Placement::Settings => "from a button in settings".to_string(), Placement::AfterUses => format!("after {} uses", w.after_uses) },
        match w.layout { Layout::Compact => "compact", Layout::FullScreen => "full screen", Layout::Sheet => "bottom sheet" },
        w.headline, w.body, w.cta, if w.product_ids.is_empty() { "all".to_string() } else { w.product_ids.join(", ") }, w.featured.as_deref().unwrap_or("none")));
    if !w.style.trim().is_empty() {
        s.push_str(&format!("Look and feel in the user's words: {}\n", w.style));
    }
    if !p.gating.points.is_empty() {
        s.push_str("\nApproved gating plan (lock exactly these, nothing else):\n");
        for g in &p.gating.points {
            s.push_str(&format!("- {} \"{}\" requires '{}'{}\n", match g.kind { GateKind::Screen => "screen", GateKind::Feature => "feature", GateKind::Button => "button" }, g.name, g.entitlement, g.file.as_ref().map(|f| format!(" ({f}{})", g.line.map(|l| format!(":{l}")).unwrap_or_default())).unwrap_or_default()));
        }
    }
    s.push_str("\nAlso add widget tests for Entitlements and ProGate using the FakePurchaseProvider. Update STUDIO.md with a short note about the purchases structure. Do not touch any secret or key file.");
    s
}

/// De vraag om een gating-plan te maken (alleen lezen; Claude geeft JSON terug en wijzigt niets).
pub fn gating_prompt(description: &str, entitlements: &[String], screens: &[(String, String)]) -> String {
    let screens: Vec<String> = screens.iter().map(|(c, f)| format!("{c} ({f})")).collect();
    format!(
        "STUDIO GATING PLAN. Do NOT change any file. Read the project and propose what goes behind the paywall.\n\nThe user says: \"{description}\"\nEntitlements that exist: {}\nScreens: {}\n\nAnswer with ONE json code block and nothing else: an array of objects {{\"kind\":\"screen|feature|button\",\"name\":\"<plain-language name the user would recognise>\",\"entitlement\":\"<one of the entitlements>\",\"file\":\"<relative path>\",\"line\":<number>}}. Use the file and line where the screen, feature or button is built, so Studio can show them. Only list things the user asked for.",
        if entitlements.is_empty() { "pro".to_string() } else { entitlements.join(", ") },
        screens.join(", ")
    )
}

/// Haalt het plan uit het antwoord van Claude (een json-blok of een kale lijst). Onbekende soorten worden overgeslagen.
pub fn parse_gating_plan(answer: &str) -> Option<Vec<GatePoint>> {
    let text = match answer.find("```") {
        Some(i) => {
            let rest = &answer[i + 3..];
            let rest = rest.strip_prefix("json").unwrap_or(rest);
            rest[..rest.find("```").unwrap_or(rest.len())].to_string()
        }
        None => answer[answer.find('[')?..=answer.rfind(']')?].to_string(),
    };
    let v: serde_json::Value = serde_json::from_str(text.trim()).ok()?;
    let arr = v.as_array()?;
    let points: Vec<GatePoint> = arr
        .iter()
        .filter_map(|o| {
            let kind = match o["kind"].as_str()? {
                "screen" => GateKind::Screen,
                "feature" => GateKind::Feature,
                "button" => GateKind::Button,
                _ => return None,
            };
            let name = o["name"].as_str()?.trim().to_string();
            if name.is_empty() {
                return None;
            }
            Some(GatePoint { kind, name, entitlement: o["entitlement"].as_str().unwrap_or("pro").to_string(), file: o["file"].as_str().map(String::from), line: o["line"].as_u64().map(|l| l as u32) })
        })
        .collect();
    (!points.is_empty()).then_some(points)
}

/// Gewone-taalzin voor wat gaat gebeuren (vóór bevestigen). Geeft de onderdelen, de UI zet er de vertaling aan.
pub fn describe(plan: &ApplyPlan) -> Vec<(&'static str, usize)> {
    let mut v = vec![];
    if !plan.create.is_empty() {
        v.push(("create", plan.create.len()));
    }
    if !plan.update.is_empty() {
        v.push(("update", plan.update.len()));
    }
    if !plan.remove.is_empty() {
        v.push(("remove", plan.remove.len()));
    }
    if plan.gate_points_added > 0 {
        v.push(("lock", plan.gate_points_added));
    }
    if plan.gate_points_removed > 0 {
        v.push(("unlock", plan.gate_points_removed));
    }
    if plan.paywall_changed {
        v.push(("paywall", 1));
    }
    if plan.provider_changed {
        v.push(("provider", 1));
    }
    v
}

/// Eén begeleide stap in een winkel (alles wat Studio niet zelf via een API kan of mag doen).
#[derive(Debug, Clone, serde::Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StoreTask {
    pub store: &'static str,
    pub product: String,
    pub full_id: String,
    pub name: String,
    pub kind: Kind,
    pub period: Option<Period>,
    pub base_price: f64,
    pub currency: String,
    pub trial_days: Option<u32>,
}

/// Wat er in de winkels aangemaakt moet worden: nieuwe producten, in elke winkel die nog niet "aangemaakt" is.
pub fn store_tasks(p: &Purchases, bundle: &str, apple: bool, google: bool) -> Vec<StoreTask> {
    let mut out = vec![];
    for pr in &p.products {
        for (store, on, status) in [("apple", apple, pr.status.apple), ("google", google, pr.status.google)] {
            if on && status < StoreStatus::Created {
                out.push(StoreTask { store, product: pr.id.clone(), full_id: full_id(bundle, &pr.id), name: pr.name.clone(), kind: pr.kind, period: pr.period, base_price: pr.base_price, currency: pr.currency.clone(), trial_days: pr.trial_days });
            }
        }
    }
    out
}

/// Zet de toegepaste staat vast (na een geslaagde code-stap) en markeert nieuwe producten als "in de app".
pub fn finalize(p: &mut Purchases) {
    for pr in &mut p.products {
        if pr.status.apple == StoreStatus::Concept {
            pr.status.apple = StoreStatus::InApp;
        }
        if pr.status.google == StoreStatus::Concept {
            pr.status.google = StoreStatus::InApp;
        }
    }
    p.applied = Some(Applied { products: p.products.clone(), gating: p.gating.clone(), paywall: p.paywall.clone(), provider: p.provider });
    p.applied_version += 1;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Purchases {
        Purchases {
            products: vec![
                Product { id: "pro".into(), kind: Kind::OneTime, name: "Pro".into(), base_price: 9.99, currency: "EUR".into(), entitlement: "pro".into(), ..Default::default() },
                Product { id: "pro_yearly".into(), kind: Kind::Subscription, name: "Pro yearly".into(), base_price: 29.99, currency: "EUR".into(), period: Some(Period::Year), trial_days: Some(7), entitlement: "pro".into(), ..Default::default() },
            ],
            gating: GatingPlan { approved: true, points: vec![GatePoint { kind: GateKind::Button, name: "Export".into(), entitlement: "pro".into(), file: Some("lib/stats.dart".into()), line: Some(12) }] },
            paywall: Paywall { headline: "Go Pro".into(), cta: "Continue".into(), featured: Some("pro_yearly".into()), ..Default::default() },
            ..Default::default()
        }
    }

    #[test]
    fn the_code_prompt_states_the_structure_and_the_hard_rules() {
        let p = sample();
        let s = codegen_prompt(&p, &plan(&p), "com.acme.app", "https://acme.app/privacy", "");
        for needle in ["com.acme.app.pro_yearly", "renews every year", "7-day free trial", "FakePurchaseProvider", "STUDIO_FAKE_PURCHASES", "NEVER hardcode an amount", "Restore purchases", "ProGate", "button \"Export\" requires 'pro' (lib/stats.dart:12)", "privacy: https://acme.app/privacy", "terms: <ask the user>", "String.fromEnvironment('REVENUECAT_SDK_KEY')", "create the whole structure"] {
            assert!(s.contains(needle), "missing {needle:?} in:\n{s}");
        }
        assert!(!s.contains("9.99") && !s.contains("29.99"), "the base prices never reach the code instructions");
    }

    #[test]
    fn an_update_only_lists_the_difference() {
        let mut p = sample();
        finalize(&mut p);
        p.products[0].description = "Now with more".into();
        p.products.push(Product { id: "tip".into(), kind: Kind::Consumable, name: "Tip".into(), base_price: 1.0, currency: "EUR".into(), ..Default::default() });
        let s = codegen_prompt(&p, &plan(&p), "com.acme.app", "", "");
        assert!(s.contains("This is an UPDATE") && s.contains("new products: tip") && s.contains("changed products: pro") && !s.contains("create the whole structure"));
        p.provider = Provider::InAppPurchase;
        assert!(codegen_prompt(&p, &plan(&p), "x.y", "", "").contains("provider changed"));
    }

    #[test]
    fn the_gating_answer_is_parsed_from_a_json_block_or_a_bare_list() {
        let a = "Here you go:\n```json\n[{\"kind\":\"screen\",\"name\":\"Statistics\",\"entitlement\":\"pro\",\"file\":\"lib/stats_screen.dart\",\"line\":7},{\"kind\":\"button\",\"name\":\"Export\"},{\"kind\":\"wat\",\"name\":\"x\"}]\n```\nDone";
        let pts = parse_gating_plan(a).unwrap();
        assert_eq!(pts.len(), 2);
        assert_eq!((pts[0].kind, pts[0].line, pts[0].file.as_deref()), (GateKind::Screen, Some(7), Some("lib/stats_screen.dart")));
        assert_eq!(pts[1].entitlement, "pro");
        assert!(parse_gating_plan("[{\"kind\":\"feature\",\"name\":\"Sync\"}]").is_some());
        assert!(parse_gating_plan("no json here").is_none() && parse_gating_plan("```json\n[]\n```").is_none());
        assert!(gating_prompt("Unlimited saves", &[], &[("HomeScreen".into(), "lib/home.dart".into())]).contains("Do NOT change any file"));
    }

    #[test]
    fn protected_paths_and_the_plain_language_summary() {
        assert!(is_protected("lib/core/purchases/entitlements.dart") && is_protected("./lib/features/paywall/paywall_screen.dart"));
        assert!(!is_protected("lib/core/theme/app_theme.dart") && !is_protected("lib/features/home/home_screen.dart"));
        let p = sample();
        assert_eq!(describe(&plan(&p)), vec![("create", 2), ("lock", 1), ("paywall", 1)]);
    }

    #[test]
    fn store_tasks_cover_only_what_is_not_yet_in_the_stores_and_finalize_marks_the_app_state() {
        let mut p = sample();
        p.products[0].status.google = StoreStatus::Created;
        let t = store_tasks(&p, "com.acme.app", true, true);
        assert_eq!(t.iter().map(|x| format!("{}:{}", x.store, x.product)).collect::<Vec<_>>(), ["apple:pro", "apple:pro_yearly", "google:pro_yearly"]);
        assert_eq!(t[0].full_id, "com.acme.app.pro");
        assert_eq!(store_tasks(&p, "x.y", false, true).len(), 1);
        finalize(&mut p);
        assert_eq!((p.applied_version, p.products[1].status.apple, p.products[0].status.google), (1, StoreStatus::InApp, StoreStatus::Created));
        assert!(p.products[1].locked(), "the id is permanent from now on");
        assert!(super::super::purchases::plan(&p).is_empty());
    }
}

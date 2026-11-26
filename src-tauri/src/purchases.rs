//! Aankopen en abonnementen: het datamodel (geen geheimen; staat in het project als `studio/purchases.json`), de regels voor
//! product-ID's, en het verschil tussen "toegepast" en "nu" waarmee "Update app" alleen verwerkt wat veranderd is.

use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    #[default]
    OneTime,
    Consumable,
    Subscription,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Period {
    Week,
    Month,
    Year,
}

/// Status van een product in één winkel. De volgorde is ook de volgorde van het proces.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Default)]
#[serde(rename_all = "camelCase")]
pub enum StoreStatus {
    #[default]
    Concept,
    InApp,
    Created,
    InReview,
    Live,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct StoreStates {
    pub apple: StoreStatus,
    pub google: StoreStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Product {
    /// Het deel achter de bundle-ID (bv. `pro_monthly`). Het volledige ID is `<bundle>.<id>` en is permanent in de winkels.
    pub id: String,
    pub kind: Kind,
    pub name: String,
    pub description: String,
    /// Alleen de basisprijs om het product aan te maken. In de app komt de prijs altijd uit de winkel.
    pub base_price: f64,
    pub currency: String,
    pub period: Option<Period>,
    pub trial_days: Option<u32>,
    /// De unlock ("entitlement") die dit product geeft, bv. `pro`.
    pub entitlement: String,
    pub status: StoreStates,
}

impl Product {
    /// Zodra het product in een winkel is aangemaakt (of toegepast) mag het ID niet meer wijzigen.
    pub fn locked(&self) -> bool {
        self.status.apple >= StoreStatus::Created || self.status.google >= StoreStatus::Created || self.status.apple == StoreStatus::InApp || self.status.google == StoreStatus::InApp
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum GateKind {
    Screen,
    Feature,
    Button,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GatePoint {
    pub kind: GateKind,
    /// Gewone taal: "Statistieken", "Exporteren".
    pub name: String,
    pub entitlement: String,
    pub file: Option<String>,
    pub line: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct GatingPlan {
    pub points: Vec<GatePoint>,
    /// De gebruiker heeft het plan in gewone taal gezien en goedgekeurd.
    pub approved: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum Placement {
    Startup,
    #[default]
    LockedFeature,
    Settings,
    AfterUses,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum Layout {
    Compact,
    #[default]
    FullScreen,
    Sheet,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Paywall {
    pub placement: Placement,
    /// Bij `AfterUses`: na hoeveel keer gebruik.
    pub after_uses: u32,
    pub layout: Layout,
    /// Het uiterlijk in eigen woorden ("rustig, veel wit").
    pub style: String,
    pub headline: String,
    pub body: String,
    pub cta: String,
    /// Welke producten (ID-delen), in volgorde.
    pub product_ids: Vec<String>,
    pub featured: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum Provider {
    /// Aanbevolen voor abonnementen (afhandeling van verlengen, herstellen, rechten).
    #[default]
    RevenueCat,
    /// Eenvoudig voor eenmalige aankopen (het officiële pakket `in_app_purchase`).
    InAppPurchase,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Applied {
    pub products: Vec<Product>,
    pub gating: GatingPlan,
    pub paywall: Paywall,
    pub provider: Provider,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Purchases {
    pub schema: u32,
    pub provider: Provider,
    pub products: Vec<Product>,
    /// Wat de gebruiker in eigen woorden achter de betaalmuur wil.
    pub paywall_description: String,
    pub gating: GatingPlan,
    pub paywall: Paywall,
    /// Teller die bij elke toepassing ophoogt; `0` = nog nooit toegepast.
    pub applied_version: u32,
    /// De laatst toegepaste staat (om alleen het verschil te verwerken).
    pub applied: Option<Applied>,
    /// Handmatig bevestigde stappen (belasting- en bankgegevens, sandbox-test enz.).
    pub confirmations: BTreeMap<String, bool>,
    /// In welke winkels de app staat.
    pub apple: bool,
    pub google: bool,
}

impl Default for Purchases {
    fn default() -> Self {
        Purchases { schema: 1, provider: Provider::default(), products: vec![], paywall_description: String::new(), gating: GatingPlan::default(), paywall: Paywall::default(), applied_version: 0, applied: None, confirmations: BTreeMap::new(), apple: true, google: true }
    }
}

// ───────────── opslag ─────────────

fn path(dir: &Path) -> std::path::PathBuf {
    dir.join("studio/purchases.json")
}

pub fn load(dir: &Path) -> AppResult<Purchases> {
    match std::fs::read_to_string(path(dir)) {
        Ok(t) => serde_json::from_str(&t).map_err(|e| AppError::Invalid(format!("studio/purchases.json: {e}"))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Purchases::default()),
        Err(e) => Err(e.into()),
    }
}

pub fn save(dir: &Path, p: &Purchases) -> AppResult<()> {
    let target = path(dir);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(target, serde_json::to_string_pretty(p).map_err(|e| AppError::Invalid(e.to_string()))? + "\n")?;
    Ok(())
}

// ───────────── regels ─────────────

/// Volledig product-ID in de winkels.
pub fn full_id(bundle_id: &str, id: &str) -> String {
    format!("{bundle_id}.{id}")
}

/// Stelt een ID voor uit naam en periode: "Pro" + jaar → `pro_yearly`.
pub fn suggest_id(name: &str, kind: Kind, period: Option<Period>) -> String {
    let mut s: String = name.to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    while s.contains("__") {
        s = s.replace("__", "_");
    }
    let mut s = s.trim_matches('_').to_string();
    if s.is_empty() || !s.chars().next().is_some_and(|c| c.is_ascii_lowercase()) {
        s = format!("product_{s}").trim_end_matches('_').to_string();
    }
    if kind == Kind::Subscription {
        s.push('_');
        s.push_str(match period {
            Some(Period::Week) => "weekly",
            Some(Period::Month) => "monthly",
            Some(Period::Year) | None => "yearly",
        });
    }
    s
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Problem {
    pub product: String,
    /// Stabiele code; de UI vertaalt.
    pub code: &'static str,
}

fn id_ok(id: &str) -> bool {
    id.len() >= 3 && id.len() <= 60 && id.chars().next().is_some_and(|c| c.is_ascii_lowercase()) && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

pub fn validate(p: &Purchases) -> Vec<Problem> {
    let mut out = vec![];
    let mut seen = std::collections::BTreeSet::new();
    for pr in &p.products {
        let bad = |code| Problem { product: pr.id.clone(), code };
        if !id_ok(&pr.id) {
            out.push(bad("id_invalid"));
        }
        if !seen.insert(pr.id.clone()) {
            out.push(bad("id_duplicate"));
        }
        if pr.name.trim().is_empty() {
            out.push(bad("name_missing"));
        }
        if pr.base_price.is_nan() || pr.base_price <= 0.0 {
            out.push(bad("price_missing"));
        }
        if pr.currency.len() != 3 || !pr.currency.chars().all(|c| c.is_ascii_uppercase()) {
            out.push(bad("currency_invalid"));
        }
        if pr.kind == Kind::Subscription && pr.period.is_none() {
            out.push(bad("period_missing"));
        }
        if pr.kind != Kind::Subscription && (pr.period.is_some() || pr.trial_days.is_some()) {
            out.push(bad("period_only_for_subscriptions"));
        }
        if pr.kind != Kind::Consumable && pr.entitlement.trim().is_empty() {
            out.push(bad("entitlement_missing"));
        }
    }
    out
}

// ───────────── verschil ─────────────

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Risk {
    /// `price_increase`, `remove_with_buyers`, `entitlement_changed`
    pub code: &'static str,
    pub product: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApplyPlan {
    pub first_time: bool,
    pub create: Vec<String>,
    pub update: Vec<String>,
    pub remove: Vec<String>,
    pub gate_points_added: usize,
    pub gate_points_removed: usize,
    pub paywall_changed: bool,
    pub provider_changed: bool,
    pub risks: Vec<Risk>,
}

impl ApplyPlan {
    pub fn is_empty(&self) -> bool {
        self.create.is_empty() && self.update.is_empty() && self.remove.is_empty() && self.gate_points_added == 0 && self.gate_points_removed == 0 && !self.paywall_changed && !self.provider_changed
    }
}

/// Wat moet er gebeuren om de laatst toegepaste staat gelijk te maken aan de huidige?
pub fn plan(now: &Purchases) -> ApplyPlan {
    let mut out = ApplyPlan { first_time: now.applied.is_none(), ..Default::default() };
    let empty = Applied::default();
    let was = now.applied.as_ref().unwrap_or(&empty);
    for pr in &now.products {
        match was.products.iter().find(|o| o.id == pr.id) {
            None => out.create.push(pr.id.clone()),
            Some(old) => {
                let changed = old.name != pr.name || old.description != pr.description || old.base_price != pr.base_price || old.currency != pr.currency || old.period != pr.period || old.trial_days != pr.trial_days || old.entitlement != pr.entitlement || old.kind != pr.kind;
                if changed {
                    out.update.push(pr.id.clone());
                }
                let live = old.status.apple >= StoreStatus::Created || old.status.google >= StoreStatus::Created;
                if live && pr.kind == Kind::Subscription && pr.base_price > old.base_price {
                    out.risks.push(Risk { code: "price_increase", product: pr.id.clone() });
                }
                if live && old.entitlement != pr.entitlement {
                    out.risks.push(Risk { code: "entitlement_changed", product: pr.id.clone() });
                }
            }
        }
    }
    for old in &was.products {
        if !now.products.iter().any(|p| p.id == old.id) {
            out.remove.push(old.id.clone());
            if old.status.apple >= StoreStatus::InReview || old.status.google >= StoreStatus::InReview {
                out.risks.push(Risk { code: "remove_with_buyers", product: old.id.clone() });
            }
        }
    }
    let key = |g: &GatePoint| (g.kind as u8, g.name.clone(), g.entitlement.clone());
    out.gate_points_added = now.gating.points.iter().filter(|g| !was.gating.points.iter().any(|o| key(o) == key(g))).count();
    out.gate_points_removed = was.gating.points.iter().filter(|o| !now.gating.points.iter().any(|g| key(o) == key(g))).count();
    out.paywall_changed = now.paywall != was.paywall && !(now.applied.is_none() && now.products.is_empty());
    out.provider_changed = now.applied.is_some() && now.provider != was.provider;
    out
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Missing {
    pub code: &'static str,
    /// Waar de gebruiker heen moet: `secrets:<integratie>`, `purchases:gating` enz.
    pub next: String,
}

/// Wat ontbreekt er nog voordat "Add to app" / "Update app" mag? `has_secret(integratie, veld)`.
pub fn missing(p: &Purchases, stores_apple: bool, stores_google: bool, has_secret: &dyn Fn(&str, &str) -> bool) -> Vec<Missing> {
    let mut m = vec![];
    if p.products.is_empty() {
        m.push(Missing { code: "no_products", next: "purchases:products".into() });
    }
    if !validate(p).is_empty() {
        m.push(Missing { code: "product_problems", next: "purchases:products".into() });
    }
    if (!p.paywall_description.trim().is_empty() || !p.gating.points.is_empty()) && !p.gating.approved {
        m.push(Missing { code: "gating_not_approved", next: "purchases:gating".into() });
    }
    if stores_google && !has_secret("google_play", "service_account") {
        m.push(Missing { code: "secret_google", next: "secrets:google_play".into() });
    }
    if stores_apple && !has_secret("apple", "private_key") {
        m.push(Missing { code: "secret_apple", next: "secrets:apple".into() });
    }
    if p.provider == Provider::RevenueCat && !has_secret("revenuecat", "public_sdk_key") {
        m.push(Missing { code: "secret_revenuecat", next: "secrets:revenuecat".into() });
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pro() -> Product {
        Product { id: "pro".into(), kind: Kind::OneTime, name: "Pro".into(), description: "Unlock everything".into(), base_price: 9.99, currency: "EUR".into(), entitlement: "pro".into(), ..Default::default() }
    }
    fn yearly() -> Product {
        Product { id: "pro_yearly".into(), kind: Kind::Subscription, name: "Pro yearly".into(), base_price: 29.99, currency: "EUR".into(), period: Some(Period::Year), trial_days: Some(7), entitlement: "pro".into(), ..Default::default() }
    }

    #[test]
    fn ids_are_suggested_consistently_and_validated() {
        assert_eq!(suggest_id("Pro", Kind::Subscription, Some(Period::Year)), "pro_yearly");
        assert_eq!(suggest_id("Pro Plus!", Kind::Subscription, Some(Period::Month)), "pro_plus_monthly");
        assert_eq!(suggest_id("Unlock", Kind::OneTime, None), "unlock");
        assert_eq!(suggest_id("3 coins", Kind::Consumable, None), "product_3_coins");
        assert_eq!(full_id("com.acme.app", "pro_yearly"), "com.acme.app.pro_yearly");
        let mut p = Purchases { products: vec![pro(), yearly()], ..Default::default() };
        assert!(validate(&p).is_empty());
        p.products[0].id = "Pro!".into();
        p.products[1].period = None;
        p.products.push(yearly());
        let codes: Vec<_> = validate(&p).iter().map(|x| x.code).collect();
        assert!(codes.contains(&"id_invalid") && codes.contains(&"period_missing") && codes.contains(&"id_duplicate"), "{codes:?}");
    }

    #[test]
    fn storage_roundtrips_without_secrets_and_defaults_for_a_new_project() {
        let d = std::env::temp_dir().join(format!("studio-purchases-{}", uuid::Uuid::new_v4()));
        assert_eq!(load(&d).unwrap(), Purchases::default());
        let p = Purchases { products: vec![pro(), yearly()], paywall_description: "Unlimited saves".into(), ..Default::default() };
        save(&d, &p).unwrap();
        assert_eq!(load(&d).unwrap(), p);
        let text = std::fs::read_to_string(d.join("studio/purchases.json")).unwrap();
        assert!(text.contains("\"basePrice\"") && text.contains("\"appliedVersion\""));
        std::fs::remove_dir_all(d).ok();
    }

    #[test]
    fn the_first_application_creates_everything_then_only_the_difference_is_processed() {
        let mut p = Purchases { products: vec![pro(), yearly()], gating: GatingPlan { approved: true, points: vec![GatePoint { kind: GateKind::Screen, name: "Statistics".into(), entitlement: "pro".into(), file: None, line: None }] }, ..Default::default() };
        let first = plan(&p);
        assert!(first.first_time && first.create == ["pro", "pro_yearly"] && first.gate_points_added == 1 && first.update.is_empty());
        // toegepast
        let mut applied_products = p.products.clone();
        for a in &mut applied_products {
            a.status = StoreStates { apple: StoreStatus::Created, google: StoreStatus::Created };
        }
        p.applied = Some(Applied { products: applied_products.clone(), gating: p.gating.clone(), paywall: p.paywall.clone(), provider: p.provider });
        p.applied_version = 1;
        assert!(plan(&p).is_empty(), "nothing changed → nothing to do");
        // alleen een tekstwijziging en een nieuw product
        p.products[0].description = "More".into();
        p.products.push(Product { id: "tip".into(), kind: Kind::Consumable, name: "Tip".into(), base_price: 1.0, currency: "EUR".into(), ..Default::default() });
        let d = plan(&p);
        assert_eq!((d.update.as_slice(), d.create.as_slice(), d.remove.len()), (&["pro".to_string()][..], &["tip".to_string()][..], 0));
        assert!(d.risks.is_empty());
    }

    #[test]
    fn risky_changes_are_flagged_in_plain_cases() {
        let mut p = Purchases { products: vec![yearly()], ..Default::default() };
        let mut done = p.products.clone();
        done[0].status = StoreStates { apple: StoreStatus::Live, google: StoreStatus::Live };
        p.applied = Some(Applied { products: done, ..Default::default() });
        p.products[0].base_price = 39.99;
        p.products[0].entitlement = "premium".into();
        let codes: Vec<_> = plan(&p).risks.iter().map(|r| r.code).collect();
        assert_eq!(codes, ["price_increase", "entitlement_changed"]);
        p.products.clear();
        let d = plan(&p);
        assert_eq!(d.remove, ["pro_yearly"]);
        assert_eq!(d.risks, vec![Risk { code: "remove_with_buyers", product: "pro_yearly".into() }]);
    }

    #[test]
    fn a_product_in_a_store_is_locked_and_concepts_are_not() {
        let mut live = pro();
        live.status.google = StoreStatus::Created;
        assert!(live.locked() && !pro().locked());
        live.status = StoreStates { apple: StoreStatus::InApp, google: StoreStatus::Concept };
        assert!(live.locked(), "once it is in the app the id is permanent too");
    }

    #[test]
    fn what_is_missing_points_to_the_right_place() {
        let mut p = Purchases { products: vec![pro()], paywall_description: "Unlimited saves".into(), ..Default::default() };
        let none = |_: &str, _: &str| false;
        let codes = |m: Vec<Missing>| m.iter().map(|x| x.code).collect::<Vec<_>>();
        assert_eq!(codes(missing(&p, true, true, &none)), ["gating_not_approved", "secret_google", "secret_apple", "secret_revenuecat"]);
        p.gating.approved = true;
        p.provider = Provider::InAppPurchase;
        let only_google = |i: &str, _: &str| i == "google_play";
        assert_eq!(codes(missing(&p, true, true, &only_google)), ["secret_apple"]);
        assert_eq!(missing(&p, false, true, &only_google), vec![]);
        assert_eq!(missing(&Purchases::default(), false, false, &none)[0].next, "purchases:products");
    }
}

//! Configuratiemotor: `studio/config.json` is de bron voor alles wat uit een instelling volgt (zonder geheimen).
//! De reconciler vergelijkt de gewenste staat met de werkelijke bestanden en past alleen het verschil toe
//! (deterministisch, idempotent, geen AI). Elk onderdeel heeft een "toepasser" die de bestanden berekent.
//! Handmatige wijzigingen (door de gebruiker of Claude) worden bij de volgende `plan` als afwijking gezien en
//! stilletjes hersteld, of: de toepasser meldt dat hij iets niet kan (`notes`).

use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const FILE: &str = "studio/config.json";
pub const FONTS: [&str; 4] = ["Inter", "Poppins", "Lora", "Roboto"];
pub const KITS: [&str; 4] = ["material", "cupertino", "rounded", "minimal"];

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ThemeCfg {
    pub seed: Option<String>,
    pub font: Option<String>,
    pub kit: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    pub version: u32,
    pub theme: ThemeCfg,
    pub data: crate::data_gen::DataCfg,
}
impl Default for Config {
    fn default() -> Self {
        Config { version: 1, theme: ThemeCfg::default(), data: Default::default() }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    pub area: String,
    pub file: String,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub changed: Vec<Change>,
    /// Een nieuwe dependency is toegevoegd: `flutter pub get` moet nog draaien.
    pub needs_pub_get: bool,
    /// Pakketten die nog met `flutter pub add` moeten (de juiste versie wordt dan door pub gekozen).
    pub pub_add: Vec<String>,
    /// Gebruikt de app het gegenereerde thema?
    pub wired: bool,
    /// Wat Studio niet zelf kon (kort, in gewone taal-sleutels voor de UI).
    pub notes: Vec<String>,
}

/// Leest de configuratie; zonder bestand begint hij leeg, met het oude `.studio/theme.json` als startpunt.
pub fn load(dir: &Path) -> Config {
    if let Ok(t) = std::fs::read_to_string(dir.join(FILE)) {
        if let Ok(c) = serde_json::from_str::<Config>(&t) {
            return c;
        }
    }
    let mut c = Config::default();
    if let Ok(t) = std::fs::read_to_string(dir.join(".studio/theme.json")) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&t) {
            c.theme.seed = v["seedColor"].as_str().map(String::from);
        }
    }
    c
}

pub fn save(dir: &Path, cfg: &Config) -> AppResult<()> {
    let p = dir.join(FILE);
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::write(p, serde_json::to_string_pretty(cfg)? + "\n")?;
    Ok(())
}

pub fn validate(cfg: &Config) -> AppResult<()> {
    if let Some(s) = &cfg.theme.seed {
        if !crate::studio_tools::valid_hex(s) {
            return Err(AppError::Invalid("seedColor must look like #4F46E5".into()));
        }
    }
    if let Some(f) = &cfg.theme.font {
        if !FONTS.contains(&f.as_str()) {
            return Err(AppError::Invalid("unknown font".into()));
        }
    }
    if let Some(k) = &cfg.theme.kit {
        if !KITS.contains(&k.as_str()) {
            return Err(AppError::Invalid("unknown ui kit".into()));
        }
    }
    Ok(())
}

// ───────────── toepasser: thema ─────────────

fn kit_args(kit: &str) -> &'static str {
    match kit {
        "rounded" => {
            "\n        cardTheme: const CardThemeData(shape: RoundedRectangleBorder(borderRadius: BorderRadius.all(Radius.circular(20)))),\
             \n        filledButtonTheme: FilledButtonThemeData(style: FilledButton.styleFrom(shape: const StadiumBorder())),\
             \n        outlinedButtonTheme: OutlinedButtonThemeData(style: OutlinedButton.styleFrom(shape: const StadiumBorder())),"
        }
        "minimal" => {
            "\n        splashFactory: NoSplash.splashFactory,\
             \n        cardTheme: const CardThemeData(elevation: 0, shape: RoundedRectangleBorder(borderRadius: BorderRadius.all(Radius.circular(4)))),\
             \n        appBarTheme: const AppBarTheme(elevation: 0, scrolledUnderElevation: 0, centerTitle: false),"
        }
        "cupertino" => {
            "\n        pageTransitionsTheme: const PageTransitionsTheme(builders: {\
             \n          TargetPlatform.android: CupertinoPageTransitionsBuilder(),\
             \n          TargetPlatform.iOS: CupertinoPageTransitionsBuilder(),\
             \n        }),\
             \n        appBarTheme: const AppBarTheme(centerTitle: true, elevation: 0, scrolledUnderElevation: 0),"
        }
        _ => "",
    }
}

pub fn theme_dart(t: &ThemeCfg) -> Option<String> {
    let seed = t.seed.as_deref()?;
    let argb = format!("0xFF{}", seed[1..].to_uppercase());
    let font = t.font.as_deref();
    let kit = kit_args(t.kit.as_deref().unwrap_or("material"));
    let mut imports = String::new();
    if t.kit.as_deref() == Some("cupertino") {
        imports.push_str("import 'package:flutter/cupertino.dart' show CupertinoPageTransitionsBuilder;\n");
    }
    if font.is_some() {
        imports.push_str("import 'package:google_fonts/google_fonts.dart';\n");
    }
    let text = font.map(|f| format!("\n        textTheme: GoogleFonts.{}TextTheme(base.textTheme),", f.to_lowercase())).unwrap_or_default();
    Some(format!(
        "import 'package:flutter/material.dart';\n{imports}\n\
         /// Generated by Studio from studio/config.json. Change it in Studio (Theme) or with the set_theme tool,\n\
         /// not by hand: manual edits here are put back the next time Studio applies the settings.\n\
         class AppTheme {{\n  \
         static const Color seed = Color({argb});\n\n  \
         static ThemeData get light => _build(Brightness.light);\n  \
         static ThemeData get dark => _build(Brightness.dark);\n\n  \
         static ThemeData _build(Brightness brightness) {{\n    \
         final base = ThemeData(colorScheme: ColorScheme.fromSeed(seedColor: seed, brightness: brightness), useMaterial3: true);\n    \
         return base.copyWith({text}{kit}\n    );\n  }}\n}}\n"
    ))
}

const GF_DEP: &str = "  google_fonts: ^6.2.1";

/// Voegt `google_fonts` toe aan de dependencies als dat er nog niet in staat.
pub fn pubspec_with_font(text: &str) -> Option<String> {
    if text.lines().any(|l| l.trim_start().starts_with("google_fonts:")) {
        return None;
    }
    let mut out = vec![];
    let mut done = false;
    for l in text.lines() {
        out.push(l.to_string());
        if !done && l.trim_end() == "dependencies:" {
            out.push(GF_DEP.to_string());
            done = true;
        }
    }
    if !done {
        return None;
    }
    let mut s = out.join("\n");
    if text.ends_with('\n') {
        s.push('\n');
    }
    Some(s)
}

/// Laat `MaterialApp` het gegenereerde thema gebruiken. `None` = al zo, of niet veilig te doen (er staat al een eigen `theme:`).
pub fn wire_theme(text: &str, import_line: &str) -> Option<String> {
    if text.contains("AppTheme.light") || !text.contains("MaterialApp") {
        return None;
    }
    let pos = text.find("MaterialApp.router(").map(|i| i + "MaterialApp.router(".len()).or_else(|| text.find("MaterialApp(").map(|i| i + "MaterialApp(".len()))?;
    // alleen als er niet al een eigen thema staat
    if text.contains("theme:") || text.contains("darkTheme:") {
        return None;
    }
    let mut s = String::with_capacity(text.len() + 120);
    s.push_str(&text[..pos]);
    s.push_str("\n      theme: AppTheme.light,\n      darkTheme: AppTheme.dark,");
    s.push_str(&text[pos..]);
    // import na de laatste bestaande import
    let at = s.rfind("\nimport ").map(|i| s[i + 1..].find('\n').map(|e| i + 1 + e + 1).unwrap_or(s.len())).unwrap_or(0);
    s.insert_str(at, &format!("{import_line}\n"));
    Some(s)
}

fn app_slug(dir: &Path) -> String {
    std::fs::read_to_string(dir.join("pubspec.yaml"))
        .ok()
        .and_then(|t| t.lines().find_map(|l| l.strip_prefix("name:").map(|n| n.trim().to_string())))
        .unwrap_or_else(|| "app".into())
}

/// Gewenste bestandsinhoud per pad, voor wat nu afwijkt van de werkelijke staat.
fn theme_diff(dir: &Path, cfg: &Config) -> (Vec<(String, String, String)>, Vec<String>) {
    let mut out = vec![]; // (area, rel path, nieuwe inhoud)
    let mut notes = vec![];
    let Some(dart) = theme_dart(&cfg.theme) else { return (out, notes) };
    let rel = crate::studio_tools::theme_file(dir).to_string();
    let want = crate::templates::adapt_material(dir, &dart);
    if std::fs::read_to_string(dir.join(&rel)).ok().as_deref() != Some(want.as_str()) {
        out.push(("theme".into(), rel.clone(), want));
    }
    let json = format!("{}\n", serde_json::to_string_pretty(&serde_json::json!({ "seedColor": cfg.theme.seed.as_deref().unwrap_or("").to_uppercase() })).unwrap_or_default());
    if std::fs::read_to_string(dir.join(".studio/theme.json")).ok().as_deref() != Some(json.as_str()) {
        out.push(("theme".into(), ".studio/theme.json".into(), json));
    }
    if cfg.theme.font.is_some() {
        if let Ok(p) = std::fs::read_to_string(dir.join("pubspec.yaml")) {
            if let Some(new) = pubspec_with_font(&p) {
                out.push(("theme".into(), "pubspec.yaml".into(), new));
            }
        }
    }
    let wired = ["lib/main.dart", "lib/app.dart"].iter().any(|f| std::fs::read_to_string(dir.join(f)).is_ok_and(|t| t.contains("AppTheme.light")));
    if !wired {
        let import = if rel == "lib/app_theme.dart" { format!("import 'package:{}/app_theme.dart';", app_slug(dir)) } else { format!("import 'package:{}/core/theme/app_theme.dart';", app_slug(dir)) };
        let mut done = false;
        for f in ["lib/app.dart", "lib/main.dart"] {
            if let Ok(t) = std::fs::read_to_string(dir.join(f)) {
                if let Some(new) = wire_theme(&t, &import) {
                    out.push(("theme".into(), f.into(), new));
                    done = true;
                    break;
                }
            }
        }
        if !done {
            notes.push("theme_not_wired".into());
        }
    }
    (out, notes)
}

fn has_dep(dir: &Path, name: &str) -> bool {
    std::fs::read_to_string(dir.join("pubspec.yaml")).is_ok_and(|t| t.lines().any(|l| l.trim_start().starts_with(&format!("{name}:"))))
}

/// Toepasser "Data & Auth": gegenereerde code + `main()` en route koppelen; pakketten worden apart gemeld.
fn data_diff(dir: &Path, cfg: &Config) -> (Vec<(String, String, String)>, Vec<String>, Vec<String>) {
    use crate::data_gen as g;
    let d = &cfg.data;
    let mut out = vec![];
    let mut pkgs: Vec<String> = vec![];
    let mut notes = g::notes(d);
    if d.provider.is_empty() || d.provider == "none" {
        return (out, vec![], pkgs);
    }
    pkgs = g::packages(d).into_iter().filter(|p| !has_dep(dir, p)).map(String::from).collect();
    let mut put = |rel: &str, content: String, only_if_missing: bool| {
        let want = crate::templates::adapt_material(dir, &content);
        let cur = std::fs::read_to_string(dir.join(rel)).ok();
        if cur.as_deref() != Some(want.as_str()) && !(only_if_missing && cur.is_some()) {
            out.push(("data".to_string(), rel.to_string(), want));
        }
    };
    if let Some(t) = g::init_dart(d) {
        put(g::INIT, t, false);
    }
    if let Some(t) = g::auth_dart(d) {
        put(g::AUTH, t, false);
    }
    if let Some(t) = g::login_dart(d) {
        put(g::LOGIN, t, true);
    }
    if d.provider == "supabase" && g::has_accounts(d) {
        put(g::DELETE_SQL, g::DELETE_USER_SQL.to_string(), false);
    }
    if let Ok(t) = std::fs::read_to_string(dir.join("lib/main.dart")) {
        match g::wire_main(&t, "import 'core/data/studio_data.dart';") {
            Some(new) => out.push(("data".into(), "lib/main.dart".into(), new)),
            None if !t.contains("initStudioData") => notes.push("data_main_not_wired".into()),
            None => {}
        }
    }
    if g::has_accounts(d) {
        if let Ok(t) = std::fs::read_to_string(dir.join("lib/core/navigation/app_router.dart")) {
            match g::wire_router(&t) {
                Some(new) => out.push(("data".into(), "lib/core/navigation/app_router.dart".into(), new)),
                None if !t.contains("LoginScreen") => notes.push("data_router_not_wired".into()),
                None => {}
            }
        }
    }
    (out, notes, pkgs)
}

/// Wat zou er veranderen? (zonder iets te schrijven.) Gebruikt voor de momentopname en de "afwijking"-melding.
pub fn plan(dir: &Path, cfg: &Config) -> Vec<Change> {
    let mut v: Vec<Change> = theme_diff(dir, cfg).0.into_iter().map(|(area, file, _)| Change { area, file }).collect();
    let (df, _, dp) = data_diff(dir, cfg);
    v.extend(df.into_iter().map(|(area, file, _)| Change { area, file }));
    if !dp.is_empty() {
        v.push(Change { area: "data".into(), file: "pubspec.yaml".into() });
    }
    let want = serde_json::to_string_pretty(cfg).map(|s| s + "\n").unwrap_or_default();
    if std::fs::read_to_string(dir.join(FILE)).ok().as_deref() != Some(want.as_str()) {
        v.push(Change { area: "config".into(), file: FILE.into() });
    }
    v
}

/// Slaat de configuratie op en past het verschil toe. Idempotent: een tweede aanroep verandert niets.
pub fn apply(dir: &Path, cfg: &Config) -> AppResult<Report> {
    validate(cfg)?;
    let mut rep = Report::default();
    let (files, notes) = theme_diff(dir, cfg);
    for (area, rel, content) in files {
        let p = dir.join(&rel);
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d)?;
        }
        std::fs::write(&p, content)?;
        if rel == "pubspec.yaml" {
            rep.needs_pub_get = true;
        }
        rep.changed.push(Change { area, file: rel });
    }
    rep.notes = notes;
    let (dfiles, dnotes, dpkgs) = data_diff(dir, cfg);
    for (area, rel, content) in dfiles {
        let p = dir.join(&rel);
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d)?;
        }
        std::fs::write(&p, content)?;
        rep.changed.push(Change { area, file: rel });
    }
    rep.notes.extend(dnotes);
    rep.pub_add = dpkgs;
    if !rep.pub_add.is_empty() {
        rep.needs_pub_get = true;
    }
    if plan(dir, cfg).iter().any(|c| c.area == "config") {
        save(dir, cfg)?;
        rep.changed.push(Change { area: "config".into(), file: FILE.into() });
    }
    rep.wired = ["lib/main.dart", "lib/app.dart"].iter().any(|f| std::fs::read_to_string(dir.join(f)).is_ok_and(|t| t.contains("AppTheme.light")));
    Ok(rep)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("studio-config-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("lib/core/theme")).unwrap();
        std::fs::write(d.join("pubspec.yaml"), "name: demo\ndependencies:\n  flutter:\n    sdk: flutter\n").unwrap();
        std::fs::write(d.join("lib/app.dart"), "import 'package:flutter/material.dart';\nclass App {\n  build() { return MaterialApp(title: 'x'); }\n}\n").unwrap();
        d
    }

    #[test]
    fn applying_twice_changes_nothing_the_second_time() {
        let d = tmp("idem");
        let mut c = Config::default();
        c.theme = ThemeCfg { seed: Some("#0d9488".into()), font: Some("Lora".into()), kit: Some("rounded".into()) };
        let r1 = apply(&d, &c).unwrap();
        assert!(r1.needs_pub_get && r1.wired && !r1.changed.is_empty());
        let app = std::fs::read_to_string(d.join("lib/app.dart")).unwrap();
        assert!(app.contains("theme: AppTheme.light") && app.contains("import 'package:demo/core/theme/app_theme.dart';"));
        let dart = std::fs::read_to_string(d.join("lib/core/theme/app_theme.dart")).unwrap();
        assert!(dart.contains("GoogleFonts.loraTextTheme") && dart.contains("StadiumBorder") && dart.contains("0xFF0D9488"));
        assert!(std::fs::read_to_string(d.join("pubspec.yaml")).unwrap().contains("google_fonts:"));
        let r2 = apply(&d, &c).unwrap();
        assert!(r2.changed.is_empty() && !r2.needs_pub_get, "{:?}", r2.changed);
        assert!(plan(&d, &c).is_empty());
    }

    #[test]
    fn manual_edits_are_seen_as_a_deviation_and_put_back() {
        let d = tmp("dev");
        let mut c = Config::default();
        c.theme.seed = Some("#4F46E5".into());
        apply(&d, &c).unwrap();
        std::fs::write(d.join("lib/core/theme/app_theme.dart"), "// hand edit\n").unwrap();
        assert_eq!(plan(&d, &c).len(), 1);
        apply(&d, &c).unwrap();
        assert!(plan(&d, &c).is_empty());
    }

    #[test]
    fn an_own_theme_is_never_overwritten_and_is_reported() {
        let d = tmp("own");
        std::fs::write(d.join("lib/app.dart"), "import 'package:flutter/material.dart';\nclass App { b() => MaterialApp(theme: ThemeData()); }\n").unwrap();
        let mut c = Config::default();
        c.theme.seed = Some("#4F46E5".into());
        let r = apply(&d, &c).unwrap();
        assert!(!r.wired && r.notes == vec!["theme_not_wired".to_string()]);
        assert!(std::fs::read_to_string(d.join("lib/app.dart")).unwrap().contains("ThemeData()"));
    }

    #[test]
    fn data_and_login_are_generated_wired_and_idempotent() {
        let d = tmp("data");
        std::fs::create_dir_all(d.join("lib/core/navigation")).unwrap();
        std::fs::write(d.join("lib/main.dart"), "import 'package:flutter/material.dart';\nimport 'app.dart';\n\nvoid main() => runApp(const App());\n").unwrap();
        std::fs::write(d.join("lib/core/navigation/app_router.dart"), "import 'package:go_router/go_router.dart';\nfinal appRouter = GoRouter(\n  routes: [\n    GoRoute(path: '/', builder: (c, s) => const X()),\n  ],\n);\n").unwrap();
        let mut c = Config::default();
        c.data = crate::data_gen::DataCfg { provider: "supabase".into(), auth: vec!["email".into(), "google".into()] };
        let r = apply(&d, &c).unwrap();
        assert_eq!(r.pub_add, vec!["supabase_flutter"]);
        assert!(r.notes.contains(&"auth_social_manual".to_string()));
        for f in ["lib/core/data/studio_data.dart", "lib/core/data/auth_repository.dart", "lib/features/auth/login_screen.dart", "studio/data/supabase_delete_user.sql"] {
            assert!(d.join(f).is_file(), "{f}");
        }
        assert!(std::fs::read_to_string(d.join("lib/main.dart")).unwrap().contains("await initStudioData();"));
        assert!(std::fs::read_to_string(d.join("lib/core/navigation/app_router.dart")).unwrap().contains("'/login'"));
        // een bewerkt inlogscherm blijft staan; gegenereerde bestanden worden teruggezet
        std::fs::write(d.join("lib/features/auth/login_screen.dart"), "// mine\n").unwrap();
        std::fs::write(d.join("lib/core/data/studio_data.dart"), "// hand edit\n").unwrap();
        let r = apply(&d, &c).unwrap();
        assert_eq!(r.changed.iter().filter(|c| c.area == "data").count(), 1);
        assert_eq!(std::fs::read_to_string(d.join("lib/features/auth/login_screen.dart")).unwrap(), "// mine\n");
        let r = apply(&d, &c).unwrap();
        assert!(r.changed.is_empty(), "{:?}", r.changed);
    }

    #[test]
    fn an_own_main_is_reported_not_rewritten() {
        let d = tmp("ownmain");
        std::fs::write(d.join("lib/main.dart"), "void main() { runApp(x); }\n").unwrap();
        let mut c = Config::default();
        c.data.provider = "local".into();
        let r = apply(&d, &c).unwrap();
        assert!(r.notes.contains(&"data_main_not_wired".to_string()));
        assert_eq!(std::fs::read_to_string(d.join("lib/main.dart")).unwrap(), "void main() { runApp(x); }\n");
    }

    #[test]
    fn remembered_brand_choices_survive_a_round_trip_and_apply_to_a_new_project() {
        let t = ThemeCfg { seed: Some("#112233".into()), font: Some("Inter".into()), kit: Some("rounded".into()) };
        let back: ThemeCfg = serde_json::from_str(&serde_json::to_string(&t).unwrap()).unwrap();
        assert_eq!(back, t);
        let d = tmp("brand");
        let mut c = load(&d);
        c.theme = back;
        let r = apply(&d, &c).unwrap();
        assert!(r.wired && std::fs::read_to_string(d.join("lib/core/theme/app_theme.dart")).unwrap().contains("0xFF112233"));
    }

    #[test]
    fn invalid_values_are_refused() {
        let mut c = Config::default();
        c.theme.seed = Some("nope".into());
        assert!(validate(&c).is_err());
        c.theme.seed = None;
        c.theme.font = Some("Comic".into());
        assert!(validate(&c).is_err());
    }

    #[test]
    fn the_old_theme_file_is_the_starting_point() {
        let d = tmp("old");
        std::fs::create_dir_all(d.join(".studio")).unwrap();
        std::fs::write(d.join(".studio/theme.json"), "{\"seedColor\":\"#112233\"}").unwrap();
        assert_eq!(load(&d).theme.seed.as_deref(), Some("#112233"));
    }

    /// Echte controle: elke combinatie van kit en lettertype moet door `flutter analyze` komen (`STUDIO_TEST_PROJECT`, een kopie van een project).
    #[test]
    #[ignore]
    fn real_generated_themes_pass_flutter_analyze() {
        let dir = std::path::PathBuf::from(std::env::var("STUDIO_TEST_PROJECT").expect("STUDIO_TEST_PROJECT"));
        let flutter = crate::proc::find("flutter").expect("flutter");
        for kit in KITS {
            let mut c = Config::default();
            c.theme = ThemeCfg { seed: Some("#0D9488".into()), font: std::env::var("STUDIO_TEST_FONT").ok(), kit: Some(kit.into()) };
            let r = apply(&dir, &c).unwrap();
            if r.needs_pub_get {
                let o = crate::proc::run(&flutter, &["pub", "get"], Some(&dir), std::time::Duration::from_secs(240)).unwrap();
                assert_eq!(o.status, Some(0), "{}", o.combined());
            }
            let o = crate::proc::run(&flutter, &["analyze", "--no-pub"], Some(&dir), std::time::Duration::from_secs(240)).unwrap();
            println!("{kit}: {}", o.combined().lines().rev().take(3).collect::<Vec<_>>().join(" | "));
            assert!(!o.combined().contains(" error "), "{kit}: {}", o.combined());
        }
    }
}

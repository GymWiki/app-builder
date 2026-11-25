//! Startbasis van elk project: feature-first (`lib/core/` + `lib/features/<scherm>/`), met Riverpod
//! voor state en go_router voor navigatie. De bestanden staan in `templates/app/`.

use crate::error::{AppError, AppResult};

pub struct TemplateFile {
    pub path: &'static str,
    pub contents: &'static str,
}

macro_rules! tf {
    ($path:literal, $src:literal) => {
        TemplateFile { path: $path, contents: include_str!($src) }
    };
}

fn common() -> Vec<TemplateFile> {
    vec![
        tf!("lib/main.dart", "../templates/app/common/lib/main.dart"),
        tf!("lib/app.dart", "../templates/app/common/lib/app.dart"),
        tf!("lib/core/theme/tokens.dart", "../templates/app/common/lib/core/theme/tokens.dart"),
        tf!("lib/core/widgets/empty_state.dart", "../templates/app/common/lib/core/widgets/empty_state.dart"),
    ]
}

#[cfg(test)]
pub const TEMPLATES: [&str; 4] = ["blank", "scoreboard", "checklist", "tracker"];

/// Packages die elk project nodig heeft (`flutter pub add`).
pub const PACKAGES: [&str; 2] = ["flutter_riverpod", "go_router"];

pub fn files(template: &str) -> Option<Vec<TemplateFile>> {
    let mut v = common();
    match template {
        "blank" => v.extend([
            tf!("lib/core/navigation/app_router.dart", "../templates/app/blank/lib/core/navigation/app_router.dart"),
            tf!("lib/features/home/home_screen.dart", "../templates/app/blank/lib/features/home/home_screen.dart"),
            tf!("lib/features/home/CLAUDE.md", "../templates/app/blank/lib/features/home/CLAUDE.md"),
        ]),
        "scoreboard" => v.extend([
            tf!("lib/core/navigation/app_router.dart", "../templates/app/scoreboard/lib/core/navigation/app_router.dart"),
            tf!("lib/core/models/player.dart", "../templates/app/scoreboard/lib/core/models/player.dart"),
            tf!("lib/features/scoreboard/scoreboard_screen.dart", "../templates/app/scoreboard/lib/features/scoreboard/scoreboard_screen.dart"),
            tf!("lib/features/scoreboard/scoreboard_controller.dart", "../templates/app/scoreboard/lib/features/scoreboard/scoreboard_controller.dart"),
            tf!("lib/features/scoreboard/widgets/player_card.dart", "../templates/app/scoreboard/lib/features/scoreboard/widgets/player_card.dart"),
            tf!("lib/features/scoreboard/CLAUDE.md", "../templates/app/scoreboard/lib/features/scoreboard/CLAUDE.md"),
        ]),
        "checklist" => v.extend([
            tf!("lib/core/navigation/app_router.dart", "../templates/app/checklist/lib/core/navigation/app_router.dart"),
            tf!("lib/core/models/checklist_item.dart", "../templates/app/checklist/lib/core/models/checklist_item.dart"),
            tf!("lib/features/checklist/checklist_screen.dart", "../templates/app/checklist/lib/features/checklist/checklist_screen.dart"),
            tf!("lib/features/checklist/checklist_controller.dart", "../templates/app/checklist/lib/features/checklist/checklist_controller.dart"),
            tf!("lib/features/checklist/widgets/checklist_item_tile.dart", "../templates/app/checklist/lib/features/checklist/widgets/checklist_item_tile.dart"),
            tf!("lib/features/checklist/CLAUDE.md", "../templates/app/checklist/lib/features/checklist/CLAUDE.md"),
        ]),
        "tracker" => v.extend([
            tf!("lib/core/navigation/app_router.dart", "../templates/app/tracker/lib/core/navigation/app_router.dart"),
            tf!("lib/core/models/entry.dart", "../templates/app/tracker/lib/core/models/entry.dart"),
            tf!("lib/features/tracker/tracker_screen.dart", "../templates/app/tracker/lib/features/tracker/tracker_screen.dart"),
            tf!("lib/features/tracker/tracker_controller.dart", "../templates/app/tracker/lib/features/tracker/tracker_controller.dart"),
            tf!("lib/features/tracker/widgets/entry_tile.dart", "../templates/app/tracker/lib/features/tracker/widgets/entry_tile.dart"),
            tf!("lib/features/tracker/CLAUDE.md", "../templates/app/tracker/lib/features/tracker/CLAUDE.md"),
        ]),
        _ => return None,
    }
    Some(v)
}

fn dart_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "\\'").replace('$', "\\$").replace(['\n', '\r'], " ")
}

const FRAMEWORK_MATERIAL: &str = "import 'package:flutter/material.dart';";
const SPLIT_MATERIAL: &str = "import 'package:material_ui/material_ui.dart';";

/// Sinds Flutter 3.50 staat Material in een eigen package (`material_ui`) en is
/// `package:flutter/material.dart` verouderd. Nieuwe projecten krijgen die dependency van
/// `flutter create`; zo ja, dan gebruiken we dezelfde import (anders de oude).
pub fn uses_material_ui(dir: &std::path::Path) -> bool {
    std::fs::read_to_string(dir.join("pubspec.yaml")).is_ok_and(|t| t.lines().any(|l| l.trim_start().starts_with("material_ui:")))
}

/// Zet de Material-import van een bestandstekst goed voor dit project.
pub fn adapt_material(dir: &std::path::Path, text: &str) -> String {
    if uses_material_ui(dir) { text.replace(FRAMEWORK_MATERIAL, SPLIT_MATERIAL) } else { text.to_string() }
}

/// Schrijft alle sjabloonbestanden in `dir` (bestaande bestanden worden overschreven: alleen voor een nieuw project).
pub fn write_all(dir: &std::path::Path, template: &str, app_name: &str) -> AppResult<()> {
    let list = files(template).ok_or_else(|| AppError::Invalid(format!("unknown template {template}")))?;
    for f in list {
        let p = dir.join(f.path);
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d)?;
        }
        std::fs::write(p, adapt_material(dir, &f.contents.replace("__APP_NAME__", &dart_escape(app_name))))?;
    }
    Ok(())
}

pub fn smoke_test(slug: &str) -> String {
    format!(
        "import 'package:flutter_riverpod/flutter_riverpod.dart';\nimport 'package:flutter_test/flutter_test.dart';\nimport 'package:{slug}/app.dart';\n\n\
         void main() {{\n  testWidgets('app starts', (WidgetTester tester) async {{\n    \
         await tester.pumpWidget(const ProviderScope(child: App()));\n    expect(find.byType(App), findsOneWidget);\n  }});\n}}\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_template_is_feature_first_with_shared_core() {
        for t in TEMPLATES {
            let list = files(t).unwrap();
            let paths: Vec<&str> = list.iter().map(|f| f.path).collect();
            assert!(paths.contains(&"lib/main.dart") && paths.contains(&"lib/app.dart"), "{t}");
            assert!(paths.contains(&"lib/core/navigation/app_router.dart") && paths.contains(&"lib/core/theme/tokens.dart"), "{t}");
            let features: Vec<&&str> = paths.iter().filter(|p| p.starts_with("lib/features/")).collect();
            assert!(!features.is_empty(), "{t} needs a feature");
            assert!(features.iter().any(|p| p.ends_with("/CLAUDE.md")), "{t}: every feature has local rules");
            assert!(features.iter().any(|p| p.ends_with("_screen.dart")), "{t}");
            // niets in lib/ buiten core/ en features/ behalve main.dart en app.dart
            assert!(paths.iter().all(|p| !p.starts_with("lib/") || p.starts_with("lib/core/") || p.starts_with("lib/features/") || *p == "lib/main.dart" || *p == "lib/app.dart"), "{t}");
        }
        assert!(files("nope").is_none());
    }

    #[test]
    fn app_uses_riverpod_gorouter_and_theme_tokens_only() {
        for t in TEMPLATES {
            let all: String = files(t).unwrap().iter().map(|f| f.contents).collect::<Vec<_>>().join("\n");
            assert!(all.contains("ProviderScope") && all.contains("MaterialApp.router") && all.contains("GoRouter"), "{t}");
            assert!(all.contains("AppTheme.light") && all.contains("AppSpacing"), "{t}");
            assert!(!all.contains("Colors.") && !all.contains("Color(0x"), "{t}: no hard-coded colors in the template");
            assert!(!all.contains("ChangeNotifier"), "{t}: one state solution only");
        }
    }

    #[test]
    fn app_name_is_escaped() {
        let d = std::env::temp_dir().join(format!("studio-tpl-{}", uuid::Uuid::new_v4()));
        write_all(&d, "blank", "Jan's $app\\").unwrap();
        let app = std::fs::read_to_string(d.join("lib/app.dart")).unwrap();
        assert!(app.contains("title: 'Jan\\'s \\$app\\\\'") && !app.contains("__APP_NAME__"));
        assert!(std::fs::read_to_string(d.join("lib/features/home/home_screen.dart")).unwrap().contains("Jan\\'s"));
        assert!(write_all(&d, "nope", "x").is_err());
    }

    #[test]
    fn follows_the_material_package_the_project_uses() {
        let d = std::env::temp_dir().join(format!("studio-mat-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("pubspec.yaml"), "dependencies:\n  flutter:\n    sdk: flutter\n  material_ui: ^1.1.0\n").unwrap();
        write_all(&d, "scoreboard", "X").unwrap();
        for f in ["lib/app.dart", "lib/features/scoreboard/scoreboard_screen.dart", "lib/core/widgets/empty_state.dart"] {
            let t = std::fs::read_to_string(d.join(f)).unwrap();
            assert!(!t.contains("package:flutter/material.dart") && (t.contains("material_ui") || !t.contains("material")), "{f}");
        }
        let old = std::env::temp_dir().join(format!("studio-mat-old-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("pubspec.yaml"), "dependencies:\n  flutter:\n    sdk: flutter\n").unwrap();
        write_all(&old, "blank", "X").unwrap();
        assert!(std::fs::read_to_string(old.join("lib/app.dart")).unwrap().contains("package:flutter/material.dart"));
    }

    #[test]
    fn smoke_test_uses_the_provider_scope() {
        let s = smoke_test("demo");
        assert!(s.contains("package:demo/app.dart") && s.contains("ProviderScope(child: App())"));
    }
}

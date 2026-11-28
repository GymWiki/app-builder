# Fonts
- Google font via package: `flutter pub add google_fonts`, then in AppTheme: `textTheme: GoogleFonts.interTextTheme(base.textTheme)` where `base` is the ThemeData built from the seed.
- Bundled font: put files in `assets/fonts/`, declare under `flutter: fonts:` in pubspec.yaml and set `fontFamily` on ThemeData.
- Never set a font on individual widgets.

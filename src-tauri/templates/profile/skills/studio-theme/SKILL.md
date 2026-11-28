---
name: studio-theme
description: Change the look of this Studio app: main color, dark mode, fonts, spacing. Use when the user asks for other colors, a different style, rounder corners or a new font.
---
# Theme
- Main color: call the studio tool `set_theme` with a hex color. It rewrites `lib/core/theme/app_theme.dart` (generated; do not edit by hand) and hot reload shows it.
- Colors in screens always come from `Theme.of(context).colorScheme`. Spacing and radius from `AppSpacing` / `AppRadius` in `lib/core/theme/tokens.dart`; add a token there instead of a number in a screen.
- Font: add it as a package or asset and set it once on the ThemeData in `AppTheme._build` (details: references/fonts.md).
- After changing the theme check both light and dark with `preview_screenshot` when the preview runs.

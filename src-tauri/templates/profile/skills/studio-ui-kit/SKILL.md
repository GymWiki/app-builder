---
name: studio-ui-kit
description: Give this Studio app a consistent look and feel with shared widgets (buttons, cards, empty states, list tiles) and an optional UI package. Use when the user asks for a style such as "more rounded", "iPhone style", "minimal", or to make screens look consistent.
---
# UI kit
- Shared widgets live in `lib/core/widgets/`. Before writing a widget, look there and in the screen index; extend an existing widget instead of copying it.
- A style change is made once: in `AppTheme` (component themes such as `cardTheme`, `filledButtonTheme`, `inputDecorationTheme`) and in the shared widgets, never per screen.
- Only add a UI package when ThemeData cannot do it (see references/kits.md), and use it through shared widgets so screens do not depend on it.

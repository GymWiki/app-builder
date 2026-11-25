# Feature: home
- Start screen of the app. Keep it a thin screen: layout only, move logic into a controller file next to it.
- Shared widgets live in lib/core/widgets; reuse them before writing new ones.
- Use `AppSpacing`/`AppRadius` and `Theme.of(context).colorScheme`; no hard-coded colors or sizes.

# Feature: scoreboard
- State lives in `scoreboard_controller.dart` (Riverpod Notifier); the screen only watches it and calls its methods.
- `Player` is a shared model in lib/core/models. Add fields there, not in the screen.
- One widget per file in `widgets/`. Use `AppSpacing`/`AppRadius` and the color scheme, no hard-coded colors.

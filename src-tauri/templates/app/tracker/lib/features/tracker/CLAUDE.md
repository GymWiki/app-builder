# Feature: tracker
- State lives in `tracker_controller.dart` (Riverpod Notifier); the screen only watches it and calls its methods.
- `Entry` is a shared model in lib/core/models. What can be tracked is the `trackedThings` list in the controller file.
- One widget per file in `widgets/`. Use `AppSpacing` and the color scheme, no hard-coded colors.

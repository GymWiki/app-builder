# Feature: checklist
- State lives in `checklist_controller.dart` (Riverpod Notifier); the screen only watches it and calls its methods.
- `ChecklistItem` is a shared model in lib/core/models. Keep it immutable.
- One widget per file in `widgets/`. Use `AppSpacing` and the color scheme, no hard-coded colors.

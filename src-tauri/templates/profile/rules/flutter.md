---
paths:
  - "lib/**/*.dart"
---
# Flutter rules for this project
- Use `const` constructors and `super.key`. Keep `build` methods small; extract widgets into their own files under the feature's `widgets/`.
- App state lives in Riverpod Notifiers. In `build` use `ref.watch`; in callbacks use `ref.read(...notifier)`. Use `setState` only for purely local UI state (a text field, an animation).
- After an `await`, check `if (!context.mounted) return;` before using `context`.
- Dispose controllers (`TextEditingController`, `AnimationController`) in `dispose()`.
- Long or unbounded lists: `ListView.builder`. Layouts that must adapt: `LayoutBuilder` or `Flexible/Expanded`, never fixed pixel widths.
- Navigation only through go_router (`context.go`, `context.push`); register every route in `lib/core/navigation/app_router.dart`.
- Models are immutable (`final` fields, `copyWith`). Async data: `AsyncNotifier` and `AsyncValue.when`, with loading and error states.
- Buttons and icons need a `tooltip` or `Semantics` label. Text must stay readable at large font sizes (no fixed-height text boxes).
- Use `debugPrint`, never `print`. No `dynamic` where a type is known. No business logic in widgets.
- Platform plugins and network calls belong in a data-layer class under `lib/core/data/`, not in screens.
- Import Material from the package the project already uses (see pubspec.yaml: `material_ui` or the older `package:flutter/material.dart`); never mix both.

---
name: studio-screen
description: Add a new screen or change an existing screen of this Studio app, following the feature-first structure (lib/features/<screen>/, controller, widgets, route). Use for any request like "add a page", "change what the settings screen shows", "add a button to the home screen".
---
# Add or change a screen
1. Look at the existing screens first (`list_screens`) and the target feature's CLAUDE.md. Reuse; do not duplicate.
2. New screen: create `lib/features/<name>/` with `<name>_screen.dart` (class `<Name>Screen`), `<name>_controller.dart` (Riverpod Notifier, only if there is state), `widgets/` and a short `CLAUDE.md`.
3. Register a `GoRoute` in `lib/core/navigation/app_router.dart` and link to it with `context.go` / `context.push` from where the user should reach it.
4. Use theme tokens and shared widgets only. Details and a template: references/structure.md.
5. Run `flutter analyze`; add or update a widget test in `test/` for new behavior.
6. If the route has parameters (`/item/:id`), add an example for this screen to `.studio/preview.json` (`{"routes": {"<Name>Screen": "/item/1"}}`) so the preview can open it.

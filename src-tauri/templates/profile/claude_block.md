<!-- studio:start -->
# __NAME__
Flutter app made with Studio for a non-programmer. Explain in plain language and keep changes small.

## Before you start
- Read STUDIO.md (what the app does, decisions so far).
- Use the studio tool `list_screens` (or `ls lib/features`) to see the screens.
- Change only what the task needs. Never read build/, .dart_tool/ or generated files.

## Stack
- Flutter + Material 3. State: Riverpod (Notifier, no code generation). Navigation: go_router.
- If a skill suggests ChangeNotifier, provider or get_it, use a Riverpod Notifier instead and keep the skill's layering.

## Structure
- lib/core/: theme (tokens.dart; app_theme.dart is generated: change colors only with the studio tool `set_theme`), models, navigation/app_router.dart, shared widgets.
- lib/features/<screen>/: <screen>_screen.dart, <screen>_controller.dart, widgets/, CLAUDE.md (local rules).
- A new screen = a new feature folder + a GoRoute in app_router.dart. Never put screens or logic in the lib/ root.
__SCOPE_RULE__
## Rules
- Colors from Theme.of(context).colorScheme; spacing and radius from AppSpacing/AppRadius. No hard-coded colors, fonts or magic numbers in screens.
- Reuse lib/core/widgets before writing a widget; shared code goes in lib/core, never copied.
- Run `flutter analyze` before you finish and fix what you broke. Keep `flutter test` passing.
- Never put secrets (API keys, passwords) in files.
- After a task add at most 3 lines to STUDIO.md under Decisions.
<!-- studio:end -->

# Project guide

This is a Flutter app built by a non-programmer with the help of an AI assistant. Read this whole file before you change anything.

## General rules

- Explain what you do in plain, friendly language. The user is not a developer.
- Keep changes small and focused. Do not refactor code that the task does not need.
- Never delete files unless the task asks for it.
- After every change run `flutter analyze` and fix every problem you caused. Run `flutter test` and keep the tests passing.
- Never put API keys, passwords or other secrets in the code or in files of the project.
- Prefer the packages that are already in `pubspec.yaml`. Ask before adding a package.
- All user-facing text is in English for now.

## Architecture

- The app uses Flutter with Material 3 and `flutter_riverpod` for state (Notifier classes, no code generation) and `go_router` for navigation.
- Shared code lives in `lib/core/`: models, logic, data (storage and controllers), navigation and shared widgets.
- Every screen lives in its own folder under `lib/features/<screen>/` with a `<screen>_screen.dart` file and optional widgets.
- Routes are declared in `lib/core/navigation/app_router.dart`. A new screen needs a new `GoRoute`.
- Colors come from `Theme.of(context).colorScheme`. Spacing and radius come from `AppSpacing` and `AppRadius` in `lib/core/theme/tokens.dart`. Do not hard-code colors, fonts or magic numbers in screens.

## Code style

- Use `const` constructors wherever possible.
- Prefer small widgets over deep nesting. Extract a widget when a build method grows past about 60 lines.
- Name files in `snake_case.dart`, classes in `UpperCamelCase`, variables and methods in `lowerCamelCase`.
- Use trailing commas so that `dart format` produces readable code.
- Keep business logic out of widgets: put it in plain Dart functions or in a controller.
- Money is always stored as an integer number of cents. Never use floating point for money.
- Dates are stored as `DateTime` and written to storage as ISO 8601 strings.

## State management

- A screen reads state with `ref.watch` and changes it through the notifier with `ref.read(provider.notifier)`.
- Never call `ref.read` inside a provider body to read data that can change: use `ref.watch`, or the provider will not update.
- Providers that depend on an id use `Provider.family`.
- Keep one source of truth. Do not copy lists into widget state.

## Navigation

- Use `context.go('/path')` for moving between screens and `context.pop()` to go back.
- Path parameters are read with `state.pathParameters['name']`.
- Put more specific routes before routes with a parameter at the same level, for example `expense/new` before `expense/:expenseId`.

## Testing

- Unit tests for plain logic go in `test/`. Widget tests use `ProviderScope` and `appRouter.go(...)` to reach a screen.
- Widget tests must finish with `pumpAndSettle()` before checking what is on screen.
- A test that changes the screen size must restore it with `addTearDown`.
- When you fix a bug, add a test that fails without the fix.

## Design

- Buttons are at least 48 logical pixels tall. Tap targets must be easy to hit.
- Layouts must work on a 320 px wide phone and with the text scaled to 150 percent. Use `SingleChildScrollView` or `Flexible` where content may not fit.
- Use clear, friendly sentences for empty states and error messages, and always say what the user can do next.
- Give icon buttons a tooltip.

## Data and storage

- Saved data has a version number. When the format changes, bump the version and keep reading older versions.
- Older data must always load. Missing values get sensible defaults.
- Unknown future versions must be refused with a clear error, never silently misread.

## Working with the user

- If a request is unclear, make the most sensible choice and say what you chose.
- When you are done, give a short summary: what changed, which files, and what the user can try.
- If something cannot be done safely, explain why and offer an alternative.
- Do not mention technical terms like providers or notifiers to the user unless they ask.

## Things to avoid

- Do not read or edit files in `build/` or `.dart_tool/`.
- Do not edit generated files.
- Do not change `pubspec.yaml` unless a task needs a package.
- Do not commit or push; the tool that runs you saves versions itself.

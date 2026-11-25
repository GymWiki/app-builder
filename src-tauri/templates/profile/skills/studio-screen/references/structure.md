# Screen template (Riverpod + go_router)
```dart
// lib/features/settings/settings_controller.dart
class SettingsController extends Notifier<bool> {
  @override
  bool build() => false;
  void toggle() => state = !state;
}
final settingsProvider = NotifierProvider<SettingsController, bool>(SettingsController.new);

// lib/features/settings/settings_screen.dart
class SettingsScreen extends ConsumerWidget {
  const SettingsScreen({super.key});
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final on = ref.watch(settingsProvider);
    return Scaffold(
      appBar: AppBar(title: const Text('Settings')),
      body: SwitchListTile(value: on, title: const Text('Option'), onChanged: (_) => ref.read(settingsProvider.notifier).toggle()),
    );
  }
}
```
Route: `GoRoute(path: '/settings', builder: (context, state) => const SettingsScreen())`.
Shared models go in `lib/core/models/`, shared widgets in `lib/core/widgets/`. Screens end in `Screen` so Studio can find them.

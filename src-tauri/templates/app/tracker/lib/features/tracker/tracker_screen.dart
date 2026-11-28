import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/empty_state.dart';
import 'tracker_controller.dart';
import 'widgets/entry_tile.dart';

class TrackerScreen extends ConsumerWidget {
  const TrackerScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final entries = ref.watch(trackerProvider);
    final controller = ref.read(trackerProvider.notifier);
    final text = Theme.of(context).textTheme;
    return Scaffold(
      appBar: AppBar(title: const Text('__APP_NAME__')),
      body: ListView(
        padding: const EdgeInsets.all(AppSpacing.md),
        children: [
          Text('Today', style: text.titleMedium),
          const SizedBox(height: AppSpacing.sm),
          Wrap(
            spacing: AppSpacing.sm,
            runSpacing: AppSpacing.sm,
            children: [
              for (final thing in trackedThings)
                ActionChip(
                  avatar: const Icon(Icons.add, size: 18),
                  label: Text('$thing · ${controller.countToday(thing)}'),
                  onPressed: () => controller.log(thing),
                ),
            ],
          ),
          const SizedBox(height: AppSpacing.lg),
          Text('History', style: text.titleMedium),
          if (entries.isEmpty)
            const SizedBox(
              height: 200,
              child: EmptyState(icon: Icons.history, message: 'Nothing logged yet. Tap a chip above.'),
            ),
          for (final e in entries) EntryTile(entry: e),
        ],
      ),
    );
  }
}

import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:material_ui/material_ui.dart';

import '../../core/data/trips_controller.dart';
import '../../core/theme/tokens.dart';
import '../../core/widgets/empty_state.dart';
import '../../core/widgets/money_text.dart';

class TripsScreen extends ConsumerWidget {
  const TripsScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final trips = ref.watch(tripsProvider);
    return Scaffold(
      appBar: AppBar(title: const Text('Trips')),
      floatingActionButton: FloatingActionButton.extended(
        onPressed: () => _newTrip(context, ref),
        icon: const Icon(Icons.add),
        label: const Text('New trip'),
      ),
      body: trips.isEmpty
          ? const EmptyState(icon: Icons.luggage, message: 'No trips yet.')
          : ListView(
              padding: const EdgeInsets.all(AppSpacing.md),
              children: [
                for (final t in trips)
                  Card(
                    child: ListTile(
                      title: Text(t.name),
                      subtitle: Text('${t.members.length} people'),
                      trailing: MoneyText(ref.watch(tripTotalProvider(t.id))),
                      onTap: () => context.go('/trip/${t.id}'),
                    ),
                  ),
              ],
            ),
    );
  }

  Future<void> _newTrip(BuildContext context, WidgetRef ref) async {
    final name = TextEditingController();
    final ok = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('New trip'),
        content: TextField(controller: name, decoration: const InputDecoration(labelText: 'Trip name'), autofocus: true),
        actions: [
          TextButton(onPressed: () => Navigator.pop(context, false), child: const Text('Cancel')),
          FilledButton(onPressed: () => Navigator.pop(context, true), child: const Text('Create')),
        ],
      ),
    );
    if (ok == true && name.text.trim().isNotEmpty) {
      ref.read(tripsProvider.notifier).addTrip(name.text.trim(), const ['Me']);
    }
  }
}

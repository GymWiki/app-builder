import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:material_ui/material_ui.dart';

import '../../core/data/trips_controller.dart';
import '../../core/theme/tokens.dart';
import '../../core/widgets/empty_state.dart';
import '../../core/widgets/money_text.dart';

class TripDetailScreen extends ConsumerWidget {
  const TripDetailScreen({super.key, required this.tripId});

  final String tripId;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final trip = ref.watch(tripProvider(tripId));
    if (trip == null) {
      return Scaffold(appBar: AppBar(), body: const EmptyState(icon: Icons.error_outline, message: 'Trip not found.'));
    }
    // Newest expense first.
    final expenses = [...trip.expenses]..sort((a, b) => b.date.compareTo(a.date));
    return Scaffold(
      appBar: AppBar(
        title: Text(trip.name),
        actions: [
          IconButton(
            tooltip: 'Balances',
            icon: const Icon(Icons.balance),
            onPressed: () => context.go('/trip/$tripId/balances'),
          ),
        ],
      ),
      floatingActionButton: FloatingActionButton.extended(
        onPressed: () => context.go('/trip/$tripId/expense/new'),
        icon: const Icon(Icons.add),
        label: const Text('Add expense'),
      ),
      body: Column(
        children: [
          Padding(
            padding: const EdgeInsets.all(AppSpacing.md),
            child: Row(
              children: [
                const Text('Total'),
                const Spacer(),
                MoneyText(trip.totalCents, style: Theme.of(context).textTheme.titleLarge),
              ],
            ),
          ),
          Expanded(
            child: expenses.isEmpty
                ? const EmptyState(icon: Icons.receipt_long, message: 'No expenses yet.')
                : ListView(
                    children: [
                      for (final e in expenses)
                        ListTile(
                          title: Text(e.title),
                          subtitle: Text('Paid by ${trip.member(e.paidBy)?.name ?? 'unknown'}'),
                          trailing: Row(
                            mainAxisSize: MainAxisSize.min,
                            children: [
                              MoneyText(e.amountCents),
                              IconButton(
                                key: Key('delete-${e.id}'),
                                tooltip: 'Delete expense',
                                icon: const Icon(Icons.delete_outline),
                                onPressed: () => ref.read(tripsProvider.notifier).removeExpense(tripId, e.id),
                              ),
                            ],
                          ),
                        ),
                    ],
                  ),
          ),
        ],
      ),
    );
  }
}

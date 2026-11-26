import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:material_ui/material_ui.dart';

import '../../core/data/trips_controller.dart';
import '../../core/logic/balances.dart';
import '../../core/theme/tokens.dart';
import '../../core/widgets/empty_state.dart';
import '../../core/widgets/money_text.dart';

class BalancesScreen extends ConsumerWidget {
  const BalancesScreen({super.key, required this.tripId});

  final String tripId;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final trip = ref.watch(tripProvider(tripId));
    if (trip == null) {
      return Scaffold(appBar: AppBar(), body: const EmptyState(icon: Icons.error_outline, message: 'Trip not found.'));
    }
    final debts = simplifyDebts(netBalances(trip));
    String name(String id) => trip.member(id)?.name ?? 'unknown';
    return Scaffold(
      appBar: AppBar(title: Text('${trip.name}: balances')),
      body: debts.isEmpty
          ? const EmptyState(icon: Icons.check_circle_outline, message: 'All settled.')
          : ListView(
              padding: const EdgeInsets.all(AppSpacing.md),
              children: [
                for (final d in debts)
                  Card(
                    child: ListTile(
                      title: Text('${name(d.from)} pays ${name(d.to)}'),
                      trailing: MoneyText(d.cents),
                    ),
                  ),
              ],
            ),
    );
  }
}

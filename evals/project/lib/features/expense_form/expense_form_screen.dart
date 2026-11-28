import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:material_ui/material_ui.dart';

import '../../core/data/trips_controller.dart';
import '../../core/models/expense.dart';
import '../../core/theme/tokens.dart';

class ExpenseFormScreen extends ConsumerStatefulWidget {
  const ExpenseFormScreen({super.key, required this.tripId});

  final String tripId;

  @override
  ConsumerState<ExpenseFormScreen> createState() => _ExpenseFormScreenState();
}

class _ExpenseFormScreenState extends ConsumerState<ExpenseFormScreen> {
  final _title = TextEditingController();
  final _amount = TextEditingController();
  String? _payer;

  @override
  void dispose() {
    _title.dispose();
    _amount.dispose();
    super.dispose();
  }

  void _save() {
    final trip = ref.read(tripProvider(widget.tripId));
    final cents = ((double.tryParse(_amount.text.replaceAll(',', '.')) ?? 0) * 100).round();
    if (trip == null || _title.text.trim().isEmpty || cents <= 0) return;
    ref.read(tripsProvider.notifier).addExpense(
          widget.tripId,
          Expense(
            id: 'e${DateTime.now().microsecondsSinceEpoch}',
            title: _title.text.trim(),
            amountCents: cents,
            paidBy: _payer ?? trip.members.first.id,
            date: DateTime.now(),
          ),
        );
    context.go('/trip/${widget.tripId}');
  }

  @override
  Widget build(BuildContext context) {
    final trip = ref.watch(tripProvider(widget.tripId));
    return Scaffold(
      appBar: AppBar(title: const Text('Add expense')),
      body: ListView(
        padding: const EdgeInsets.all(AppSpacing.md),
        children: [
          TextField(controller: _title, decoration: const InputDecoration(labelText: 'What was it for?')),
          const SizedBox(height: AppSpacing.md),
          TextField(
            controller: _amount,
            keyboardType: const TextInputType.numberWithOptions(decimal: true),
            decoration: const InputDecoration(labelText: 'Amount'),
          ),
          const SizedBox(height: AppSpacing.md),
          if (trip != null)
            DropdownButtonFormField<String>(
              initialValue: _payer ?? trip.members.first.id,
              decoration: const InputDecoration(labelText: 'Paid by'),
              items: [for (final m in trip.members) DropdownMenuItem(value: m.id, child: Text(m.name))],
              onChanged: (v) => setState(() => _payer = v),
            ),
          const SizedBox(height: AppSpacing.lg),
          FilledButton(onPressed: _save, child: const Text('Save')),
        ],
      ),
    );
  }
}

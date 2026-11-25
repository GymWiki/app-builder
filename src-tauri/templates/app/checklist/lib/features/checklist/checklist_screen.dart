import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/empty_state.dart';
import 'checklist_controller.dart';
import 'widgets/checklist_item_tile.dart';

class ChecklistScreen extends ConsumerStatefulWidget {
  const ChecklistScreen({super.key});

  @override
  ConsumerState<ChecklistScreen> createState() => _ChecklistScreenState();
}

class _ChecklistScreenState extends ConsumerState<ChecklistScreen> {
  final TextEditingController _input = TextEditingController();

  @override
  void dispose() {
    _input.dispose();
    super.dispose();
  }

  void _add() {
    ref.read(checklistProvider.notifier).add(_input.text);
    _input.clear();
  }

  @override
  Widget build(BuildContext context) {
    final items = ref.watch(checklistProvider);
    final controller = ref.read(checklistProvider.notifier);
    return Scaffold(
      appBar: AppBar(title: const Text('__APP_NAME__')),
      body: Column(
        children: [
          Padding(
            padding: const EdgeInsets.all(AppSpacing.md),
            child: Row(
              children: [
                Expanded(
                  child: TextField(
                    controller: _input,
                    decoration: const InputDecoration(labelText: 'New item', border: OutlineInputBorder()),
                    onSubmitted: (_) => _add(),
                  ),
                ),
                const SizedBox(width: AppSpacing.sm),
                IconButton.filled(tooltip: 'Add item', icon: const Icon(Icons.add), onPressed: _add),
              ],
            ),
          ),
          Expanded(
            child: items.isEmpty
                ? const EmptyState(icon: Icons.check_circle_outline, message: 'Nothing to do. Nice!')
                : ListView.builder(
                    itemCount: items.length,
                    itemBuilder: (context, index) => ChecklistItemTile(
                      item: items[index],
                      onToggle: () => controller.toggle(index),
                      onRemove: () => controller.removeAt(index),
                    ),
                  ),
          ),
        ],
      ),
    );
  }
}

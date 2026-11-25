import 'package:flutter/material.dart';

import '../../../core/models/checklist_item.dart';

class ChecklistItemTile extends StatelessWidget {
  const ChecklistItemTile({super.key, required this.item, required this.onToggle, required this.onRemove});

  final ChecklistItem item;
  final VoidCallback onToggle;
  final VoidCallback onRemove;

  @override
  Widget build(BuildContext context) {
    return Dismissible(
      key: ObjectKey(item),
      background: ColoredBox(color: Theme.of(context).colorScheme.errorContainer),
      onDismissed: (_) => onRemove(),
      child: CheckboxListTile(
        value: item.done,
        title: Text(
          item.title,
          style: TextStyle(decoration: item.done ? TextDecoration.lineThrough : null),
        ),
        onChanged: (_) => onToggle(),
      ),
    );
  }
}

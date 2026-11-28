import 'package:flutter/material.dart';

import '../../../core/models/entry.dart';

class EntryTile extends StatelessWidget {
  const EntryTile({super.key, required this.entry});

  final Entry entry;

  String _two(int n) => n.toString().padLeft(2, '0');

  @override
  Widget build(BuildContext context) {
    return ListTile(
      dense: true,
      leading: const Icon(Icons.check_circle_outline),
      title: Text(entry.label),
      trailing: Text('${_two(entry.time.hour)}:${_two(entry.time.minute)}'),
    );
  }
}

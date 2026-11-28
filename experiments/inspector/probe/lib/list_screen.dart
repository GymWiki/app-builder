import 'package:flutter/material.dart';
import 'core/primary_button.dart';

class ListScreen extends StatelessWidget {
  const ListScreen({super.key});

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('Items')),
      body: Column(
        children: [
          Expanded(
            child: ListView.builder(
              itemCount: 40,
              itemBuilder: (context, i) => ListTile(
                title: Text('Item $i'),
                subtitle: Text('Details for item $i'),
                trailing: IconButton(icon: const Icon(Icons.delete), onPressed: () {}),
              ),
            ),
          ),
          Padding(
            padding: const EdgeInsets.all(8),
            child: PrimaryButton(label: 'Add item', onPressed: () {}),
          ),
        ],
      ),
    );
  }
}

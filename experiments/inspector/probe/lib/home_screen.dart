import 'package:flutter/material.dart';
import 'core/primary_button.dart';
import 'list_screen.dart';
import 'settings_screen.dart';

class HomeScreen extends StatelessWidget {
  const HomeScreen({super.key});

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('Home')),
      body: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            const Text('Welcome back', style: TextStyle(fontSize: 28)),
            const SizedBox(height: 8),
            const Text('Pick something to do today.'),
            const SizedBox(height: 24),
            PrimaryButton(
              label: 'Open list',
              onPressed: () => Navigator.push(context, MaterialPageRoute(builder: (_) => const ListScreen())),
            ),
            const SizedBox(height: 12),
            PrimaryButton(
              label: 'Settings',
              onPressed: () => Navigator.push(context, MaterialPageRoute(builder: (_) => const SettingsScreen())),
            ),
            const SizedBox(height: 12),
            OutlinedButton(
              onPressed: () => showDialog<void>(
                context: context,
                builder: (ctx) => AlertDialog(
                  title: const Text('Hello there'),
                  content: const Text('This is a dialog.'),
                  actions: [
                    PrimaryButton(label: 'Close', onPressed: () => Navigator.pop(ctx)),
                  ],
                ),
              ),
              child: const Text('Open dialog'),
            ),
            const SizedBox(height: 24),
            const Card(
              child: ListTile(
                leading: Icon(Icons.star),
                title: Text('Featured'),
                subtitle: Text('A highlighted card'),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

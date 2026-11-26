import 'package:flutter/material.dart';
import 'core/primary_button.dart';

class SettingsScreen extends StatefulWidget {
  const SettingsScreen({super.key});

  @override
  State<SettingsScreen> createState() => _SettingsScreenState();
}

class _SettingsScreenState extends State<SettingsScreen> {
  bool notifications = true;
  bool dark = false;

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('Settings')),
      body: ListView(
        padding: const EdgeInsets.all(16),
        children: [
          SwitchListTile(
            title: const Text('Notifications'),
            value: notifications,
            onChanged: (v) => setState(() => notifications = v),
          ),
          SwitchListTile(
            title: const Text('Dark mode'),
            value: dark,
            onChanged: (v) => setState(() => dark = v),
          ),
          const SizedBox(height: 16),
          const Text('Version 1.0.0'),
          const SizedBox(height: 24),
          PrimaryButton(label: 'Save', onPressed: () {}),
        ],
      ),
    );
  }
}

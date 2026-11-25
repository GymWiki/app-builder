import 'package:flutter/material.dart';

import '../../core/widgets/empty_state.dart';

class HomeScreen extends StatelessWidget {
  const HomeScreen({super.key});

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('__APP_NAME__')),
      body: const EmptyState(
        icon: Icons.auto_awesome,
        message: 'Your app starts here.\nTell Studio what it should do.',
      ),
    );
  }
}

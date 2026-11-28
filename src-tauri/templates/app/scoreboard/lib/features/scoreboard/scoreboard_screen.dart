import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/theme/tokens.dart';
import 'scoreboard_controller.dart';
import 'widgets/player_card.dart';

class ScoreboardScreen extends ConsumerWidget {
  const ScoreboardScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final players = ref.watch(scoreboardProvider);
    final controller = ref.read(scoreboardProvider.notifier);
    return Scaffold(
      appBar: AppBar(
        title: const Text('__APP_NAME__'),
        actions: [
          IconButton(
            tooltip: 'Reset scores',
            icon: const Icon(Icons.restart_alt),
            onPressed: controller.reset,
          ),
        ],
      ),
      body: Padding(
        padding: const EdgeInsets.all(AppSpacing.md),
        child: Column(
          children: [
            for (var i = 0; i < players.length; i++)
              Expanded(
                child: PlayerCard(
                  player: players[i],
                  onChange: (delta) => controller.change(i, delta),
                ),
              ),
          ],
        ),
      ),
    );
  }
}

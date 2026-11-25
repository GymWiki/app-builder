import 'package:flutter/material.dart';

import '../../../core/models/player.dart';
import '../../../core/theme/tokens.dart';

class PlayerCard extends StatelessWidget {
  const PlayerCard({super.key, required this.player, required this.onChange});

  final Player player;
  final void Function(int delta) onChange;

  @override
  Widget build(BuildContext context) {
    final text = Theme.of(context).textTheme;
    return Card(
      margin: const EdgeInsets.only(bottom: AppSpacing.md),
      child: Column(
        mainAxisAlignment: MainAxisAlignment.center,
        children: [
          Text(player.name, style: text.titleLarge),
          Text('${player.score}', style: text.displayLarge),
          Row(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              IconButton.filledTonal(
                tooltip: 'Minus one',
                icon: const Icon(Icons.remove),
                onPressed: () => onChange(-1),
              ),
              const SizedBox(width: AppSpacing.md),
              IconButton.filled(
                tooltip: 'Plus one',
                icon: const Icon(Icons.add),
                onPressed: () => onChange(1),
              ),
            ],
          ),
        ],
      ),
    );
  }
}

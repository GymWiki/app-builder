import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/models/player.dart';

class ScoreboardController extends Notifier<List<Player>> {
  @override
  List<Player> build() => const [Player('Player 1', 0), Player('Player 2', 0)];

  void change(int index, int delta) {
    state = [
      for (var i = 0; i < state.length; i++)
        if (i == index) state[i].copyWith(score: state[i].score + delta) else state[i],
    ];
  }

  void reset() {
    state = [for (final p in state) p.copyWith(score: 0)];
  }
}

final scoreboardProvider = NotifierProvider<ScoreboardController, List<Player>>(ScoreboardController.new);

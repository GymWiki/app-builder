class Player {
  const Player(this.name, this.score);

  final String name;
  final int score;

  Player copyWith({int? score}) => Player(name, score ?? this.score);
}

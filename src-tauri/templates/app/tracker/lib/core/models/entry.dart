class Entry {
  const Entry(this.label, this.time);

  final String label;
  final DateTime time;

  bool isOn(DateTime day) => time.year == day.year && time.month == day.month && time.day == day.day;
}

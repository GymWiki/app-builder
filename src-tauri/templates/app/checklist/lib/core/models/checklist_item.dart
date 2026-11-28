class ChecklistItem {
  const ChecklistItem(this.title, {this.done = false});

  final String title;
  final bool done;

  ChecklistItem toggled() => ChecklistItem(title, done: !done);
}

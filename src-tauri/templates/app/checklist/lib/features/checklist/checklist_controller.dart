import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/models/checklist_item.dart';

class ChecklistController extends Notifier<List<ChecklistItem>> {
  @override
  List<ChecklistItem> build() => const [ChecklistItem('Try this app'), ChecklistItem('Add your first item')];

  void add(String title) {
    final text = title.trim();
    if (text.isEmpty) return;
    state = [...state, ChecklistItem(text)];
  }

  void toggle(int index) {
    state = [
      for (var i = 0; i < state.length; i++) if (i == index) state[i].toggled() else state[i],
    ];
  }

  void removeAt(int index) {
    state = [
      for (var i = 0; i < state.length; i++) if (i != index) state[i],
    ];
  }
}

final checklistProvider = NotifierProvider<ChecklistController, List<ChecklistItem>>(ChecklistController.new);

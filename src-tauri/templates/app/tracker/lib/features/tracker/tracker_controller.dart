import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/models/entry.dart';

class TrackerController extends Notifier<List<Entry>> {
  @override
  List<Entry> build() => const [];

  void log(String label) => state = [Entry(label, DateTime.now()), ...state];

  int countToday(String label) {
    final now = DateTime.now();
    return state.where((e) => e.label == label && e.isOn(now)).length;
  }
}

final trackerProvider = NotifierProvider<TrackerController, List<Entry>>(TrackerController.new);

/// What can be tracked. Change this list to track other things.
const trackedThings = ['Water', 'Walk', 'Read'];

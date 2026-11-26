import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../models/expense.dart';
import '../models/member.dart';
import '../models/trip.dart';

List<Trip> seedTrips() => [
      Trip(
        id: 't1',
        name: 'Lisbon',
        members: const [Member('m1', 'Ana'), Member('m2', 'Ben'), Member('m3', 'Cleo')],
        expenses: [
          Expense(id: 'e1', title: 'Dinner', amountCents: 9000, paidBy: 'm1', date: DateTime(2026, 5, 2)),
          Expense(id: 'e2', title: 'Taxi', amountCents: 3000, paidBy: 'm2', date: DateTime(2026, 5, 3)),
          Expense(id: 'e3', title: 'Museum', amountCents: 4500, paidBy: 'm3', date: DateTime(2026, 5, 4)),
        ],
      ),
      Trip(
        id: 't2',
        name: 'Berlin',
        members: const [Member('n1', 'Ana'), Member('n2', 'Ben')],
        expenses: [Expense(id: 'e4', title: 'Hostel', amountCents: 12000, paidBy: 'n1', date: DateTime(2026, 6, 1))],
      ),
    ];

class TripsController extends Notifier<List<Trip>> {
  @override
  List<Trip> build() => seedTrips();

  void addTrip(String name, List<String> memberNames) {
    final id = 't${state.length + 1}-${name.hashCode.abs()}';
    final members = [for (var i = 0; i < memberNames.length; i++) Member('$id-m$i', memberNames[i])];
    state = [...state, Trip(id: id, name: name, members: members)];
  }

  void addExpense(String tripId, Expense expense) {
    state = [
      for (final t in state)
        if (t.id == tripId) t.copyWith(expenses: [...t.expenses, expense]) else t,
    ];
  }

  void removeExpense(String tripId, String expenseId) {
    state = [
      for (final t in state)
        if (t.id == tripId) t.copyWith(expenses: [for (final e in t.expenses) if (e.id != expenseId) e]) else t,
    ];
  }
}

final tripsProvider = NotifierProvider<TripsController, List<Trip>>(TripsController.new);

/// One trip by id (null if it does not exist).
final tripProvider = Provider.family<Trip?, String>((ref, id) {
  for (final t in ref.watch(tripsProvider)) {
    if (t.id == id) return t;
  }
  return null;
});

/// Total of a trip, for the lists.
final tripTotalProvider = Provider.family<int, String>((ref, id) => ref.watch(tripProvider(id))?.totalCents ?? 0);

import '../models/trip.dart';

/// One person who has to pay another person.
class Debt {
  const Debt(this.from, this.to, this.cents);

  final String from;
  final String to;
  final int cents;
}

/// Positive = the member is owed money, negative = the member owes money.
Map<String, int> netBalances(Trip trip) {
  final share = trip.totalCents ~/ trip.members.length;
  final net = {for (final m in trip.members) m.id: -share};
  for (final e in trip.expenses) {
    net[e.paidBy] = (net[e.paidBy] ?? 0) + e.amountCents;
  }
  return net;
}

/// Turns balances into the fewest simple payments (largest debtor pays largest creditor first).
List<Debt> simplifyDebts(Map<String, int> net) {
  final debtors = [for (final e in net.entries) if (e.value < 0) MapEntry(e.key, -e.value)]..sort((a, b) => b.value.compareTo(a.value));
  final creditors = [for (final e in net.entries) if (e.value > 0) MapEntry(e.key, e.value)]..sort((a, b) => b.value.compareTo(a.value));
  final debts = <Debt>[];
  var i = 0;
  var j = 0;
  var owed = debtors.isEmpty ? 0 : debtors[0].value;
  var due = creditors.isEmpty ? 0 : creditors[0].value;
  while (i < debtors.length && j < creditors.length) {
    final pay = owed < due ? owed : due;
    if (pay > 0) debts.add(Debt(debtors[i].key, creditors[j].key, pay));
    owed -= pay;
    due -= pay;
    if (owed == 0 && ++i < debtors.length) owed = debtors[i].value;
    if (due == 0 && ++j < creditors.length) due = creditors[j].value;
  }
  return debts;
}

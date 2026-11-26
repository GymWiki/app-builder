import 'package:flutter_test/flutter_test.dart';
import 'package:eval_demo/core/data/trips_controller.dart';
import 'package:eval_demo/core/logic/balances.dart';

void main() {
  test('seed trip: Ben and Cleo pay Ana', () {
    final trip = seedTrips().first;
    final net = netBalances(trip);
    expect(net['m1'], 3500);
    expect(net['m2'], -2500);
    expect(net['m3'], -1000);
    final debts = simplifyDebts(net);
    expect([for (final d in debts) '${d.from}>${d.to}:${d.cents}'], ['m2>m1:2500', 'm3>m1:1000']);
  });
}

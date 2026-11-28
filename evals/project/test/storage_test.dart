import 'package:flutter_test/flutter_test.dart';
import 'package:eval_demo/core/data/trip_storage.dart';
import 'package:eval_demo/core/data/trips_controller.dart';

void main() {
  test('saved trips can be read back', () {
    final storage = JsonTripStorage();
    final trips = storage.decode(storage.encode(seedTrips()));
    expect(trips.length, 2);
    expect(trips.first.expenses.first.title, 'Dinner');
  });
}

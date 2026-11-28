import 'dart:convert';

import '../models/trip.dart';

/// Turns trips into text for saving on the device, and back.
abstract class TripStorage {
  String encode(List<Trip> trips);
  List<Trip> decode(String text);
}

class JsonTripStorage implements TripStorage {
  /// Bump this when the saved format changes, and keep reading older versions.
  static const int version = 1;

  @override
  String encode(List<Trip> trips) => jsonEncode({
        'storageVersion': version,
        'trips': [for (final t in trips) t.toJson()],
      });

  @override
  List<Trip> decode(String text) {
    final map = (jsonDecode(text) as Map).cast<String, Object?>();
    final v = map['storageVersion'];
    if (v != 1) throw FormatException('Unsupported storage version: $v');
    return [for (final t in map['trips'] as List) Trip.fromJson((t as Map).cast<String, Object?>())];
  }
}

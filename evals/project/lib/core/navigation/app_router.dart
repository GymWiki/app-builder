import 'package:go_router/go_router.dart';

import '../../features/balances/balances_screen.dart';
import '../../features/expense_form/expense_form_screen.dart';
import '../../features/trip_detail/trip_detail_screen.dart';
import '../../features/trips/trips_screen.dart';

/// All routes of the app. Add a GoRoute here for every new screen.
final appRouter = GoRouter(
  routes: [
    GoRoute(path: '/', builder: (context, state) => const TripsScreen()),
    GoRoute(
      path: '/trip/:id',
      builder: (context, state) => TripDetailScreen(tripId: state.pathParameters['id']!),
      routes: [
        GoRoute(
          path: 'expense/new',
          builder: (context, state) => ExpenseFormScreen(tripId: state.pathParameters['id']!),
        ),
        GoRoute(
          path: 'balances',
          builder: (context, state) => BalancesScreen(tripId: state.pathParameters['id']!),
        ),
      ],
    ),
  ],
);

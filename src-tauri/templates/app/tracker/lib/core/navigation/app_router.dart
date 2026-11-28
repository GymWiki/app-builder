import 'package:go_router/go_router.dart';

import '../../features/tracker/tracker_screen.dart';

/// All routes of the app. Add a GoRoute here for every new screen.
final appRouter = GoRouter(
  routes: [
    GoRoute(path: '/', builder: (context, state) => const TrackerScreen()),
  ],
);

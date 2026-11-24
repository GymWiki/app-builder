import 'package:flutter/material.dart';

void main() => runApp(const MyApp());

class MyApp extends StatelessWidget {
  const MyApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: '__APP_NAME__',
      debugShowCheckedModeBanner: false,
      theme: ThemeData(
        colorScheme: ColorScheme.fromSeed(seedColor: Colors.indigo),
        useMaterial3: true,
      ),
      darkTheme: ThemeData(
        colorScheme: ColorScheme.fromSeed(
          seedColor: Colors.indigo,
          brightness: Brightness.dark,
        ),
        useMaterial3: true,
      ),
      home: const TrackerScreen(),
    );
  }
}

class Entry {
  Entry(this.label, this.time);
  final String label;
  final DateTime time;
}

class TrackerScreen extends StatefulWidget {
  const TrackerScreen({super.key});

  @override
  State<TrackerScreen> createState() => _TrackerScreenState();
}

class _TrackerScreenState extends State<TrackerScreen> {
  final List<String> _things = ['Water', 'Walk', 'Read'];
  final List<Entry> _entries = [];

  int _countToday(String label) {
    final now = DateTime.now();
    return _entries
        .where((e) =>
            e.label == label &&
            e.time.year == now.year &&
            e.time.month == now.month &&
            e.time.day == now.day)
        .length;
  }

  void _log(String label) {
    setState(() => _entries.insert(0, Entry(label, DateTime.now())));
  }

  String _two(int n) => n.toString().padLeft(2, '0');

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('__APP_NAME__')),
      body: ListView(
        padding: const EdgeInsets.all(16),
        children: [
          Text('Today', style: Theme.of(context).textTheme.titleMedium),
          const SizedBox(height: 8),
          Wrap(
            spacing: 8,
            runSpacing: 8,
            children: [
              for (final thing in _things)
                ActionChip(
                  avatar: const Icon(Icons.add, size: 18),
                  label: Text('$thing · ${_countToday(thing)}'),
                  onPressed: () => _log(thing),
                ),
            ],
          ),
          const SizedBox(height: 24),
          Text('History', style: Theme.of(context).textTheme.titleMedium),
          if (_entries.isEmpty)
            const Padding(
              padding: EdgeInsets.symmetric(vertical: 24),
              child: Text('Nothing logged yet. Tap a chip above.'),
            ),
          for (final e in _entries)
            ListTile(
              dense: true,
              leading: const Icon(Icons.check_circle_outline),
              title: Text(e.label),
              trailing: Text('${_two(e.time.hour)}:${_two(e.time.minute)}'),
            ),
        ],
      ),
    );
  }
}

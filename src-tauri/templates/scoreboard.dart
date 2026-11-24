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
      home: const ScoreboardScreen(),
    );
  }
}

class ScoreboardScreen extends StatefulWidget {
  const ScoreboardScreen({super.key});

  @override
  State<ScoreboardScreen> createState() => _ScoreboardScreenState();
}

class _ScoreboardScreenState extends State<ScoreboardScreen> {
  final List<String> _names = ['Player 1', 'Player 2'];
  final List<int> _scores = [0, 0];

  void _change(int index, int delta) {
    setState(() => _scores[index] += delta);
  }

  void _reset() {
    setState(() {
      for (var i = 0; i < _scores.length; i++) {
        _scores[i] = 0;
      }
    });
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('__APP_NAME__'),
        actions: [
          IconButton(
            tooltip: 'Reset scores',
            icon: const Icon(Icons.restart_alt),
            onPressed: _reset,
          ),
        ],
      ),
      body: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          children: [
            for (var i = 0; i < _names.length; i++)
              Expanded(
                child: Card(
                  margin: const EdgeInsets.only(bottom: 16),
                  child: Column(
                    mainAxisAlignment: MainAxisAlignment.center,
                    children: [
                      Text(_names[i], style: Theme.of(context).textTheme.titleLarge),
                      Text(
                        '${_scores[i]}',
                        style: Theme.of(context).textTheme.displayLarge,
                      ),
                      Row(
                        mainAxisAlignment: MainAxisAlignment.center,
                        children: [
                          IconButton.filledTonal(
                            tooltip: 'Minus one',
                            icon: const Icon(Icons.remove),
                            onPressed: () => _change(i, -1),
                          ),
                          const SizedBox(width: 16),
                          IconButton.filled(
                            tooltip: 'Plus one',
                            icon: const Icon(Icons.add),
                            onPressed: () => _change(i, 1),
                          ),
                        ],
                      ),
                    ],
                  ),
                ),
              ),
          ],
        ),
      ),
    );
  }
}

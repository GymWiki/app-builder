// Studio hook (generated): formats the Dart file Claude just edited. Cheap, and never fails an edit.
import 'dart:convert';
import 'dart:io';

Future<void> main() async {
  try {
    final input = await utf8.decoder.bind(stdin).join();
    final data = jsonDecode(input) as Map<String, dynamic>;
    final toolInput = data['tool_input'];
    final path = toolInput is Map<String, dynamic> ? toolInput['file_path'] as String? : null;
    if (path == null || !path.endsWith('.dart')) return;
    if (path.endsWith('.g.dart') || path.endsWith('.freezed.dart')) return;
    final file = File(path);
    if (!file.existsSync()) return;
    final root = Platform.environment['CLAUDE_PROJECT_DIR'];
    String norm(String p) => p.replaceAll('\\', '/');
    if (root != null && !norm(file.absolute.path).startsWith(norm(Directory(root).absolute.path))) return;
    await Process.run('dart', ['format', path], runInShell: Platform.isWindows);
  } catch (_) {
    // A formatting problem must never block the edit.
  }
}

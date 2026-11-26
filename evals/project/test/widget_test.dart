import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:eval_demo/app.dart';

void main() {
  testWidgets('shows the trips', (WidgetTester tester) async {
    await tester.pumpWidget(const ProviderScope(child: App()));
    await tester.pumpAndSettle();
    expect(find.text('Lisbon'), findsOneWidget);
    expect(find.text('Berlin'), findsOneWidget);
  });
}

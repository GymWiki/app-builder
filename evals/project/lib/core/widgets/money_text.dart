import 'package:material_ui/material_ui.dart';

import '../logic/format.dart';

/// An amount in cents, shown as text.
class MoneyText extends StatelessWidget {
  const MoneyText(this.cents, {super.key, this.style});

  final int cents;
  final TextStyle? style;

  @override
  Widget build(BuildContext context) => Text(formatCents(cents), style: style);
}

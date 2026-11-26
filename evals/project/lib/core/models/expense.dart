class Expense {
  const Expense({
    required this.id,
    required this.title,
    required this.amountCents,
    required this.paidBy,
    required this.date,
  });

  final String id;
  final String title;

  /// Amount in cents, so money math never uses floating point.
  final int amountCents;

  /// Member id of whoever paid.
  final String paidBy;
  final DateTime date;

  Map<String, Object?> toJson() => {
        'id': id,
        'title': title,
        'amountCents': amountCents,
        'paidBy': paidBy,
        'date': date.toIso8601String(),
      };

  factory Expense.fromJson(Map<String, Object?> json) => Expense(
        id: json['id'] as String,
        title: json['title'] as String,
        amountCents: json['amountCents'] as int,
        paidBy: json['paidBy'] as String,
        date: DateTime.parse(json['date'] as String),
      );
}

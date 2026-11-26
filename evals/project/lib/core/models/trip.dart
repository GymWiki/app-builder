import 'expense.dart';
import 'member.dart';

class Trip {
  const Trip({
    required this.id,
    required this.name,
    required this.members,
    this.expenses = const [],
  });

  final String id;
  final String name;
  final List<Member> members;
  final List<Expense> expenses;

  int get totalCents => expenses.fold(0, (sum, e) => sum + e.amountCents);

  Member? member(String id) {
    for (final m in members) {
      if (m.id == id) return m;
    }
    return null;
  }

  Trip copyWith({List<Expense>? expenses}) => Trip(id: id, name: name, members: members, expenses: expenses ?? this.expenses);

  Map<String, Object?> toJson() => {
        'id': id,
        'name': name,
        'members': [for (final m in members) m.toJson()],
        'expenses': [for (final e in expenses) e.toJson()],
      };

  factory Trip.fromJson(Map<String, Object?> json) => Trip(
        id: json['id'] as String,
        name: json['name'] as String,
        members: [for (final m in json['members'] as List) Member.fromJson((m as Map).cast<String, Object?>())],
        expenses: [for (final e in json['expenses'] as List) Expense.fromJson((e as Map).cast<String, Object?>())],
      );
}

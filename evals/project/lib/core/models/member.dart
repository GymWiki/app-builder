class Member {
  const Member(this.id, this.name);

  final String id;
  final String name;

  Map<String, Object?> toJson() => {'id': id, 'name': name};

  factory Member.fromJson(Map<String, Object?> json) => Member(json['id'] as String, json['name'] as String);
}

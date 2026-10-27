// Provider library descriptors are ephemeral guards, never local backup data.
class NativeLibraryVm {
  final Map<String, dynamic> guard;
  NativeLibraryVm._(this.guard);
  String get label => guard['label'] as String;
  String get authority => guard['authority'] as String;
  bool get connected => guard['connected'] == true;
  bool get complete => guard['complete'] == true;
  int get capacity => guard['capacity'] as int;
  String get limitations => guard['limitations'] as String;
  List<Map<String, dynamic>> get slots =>
      (guard['slots'] as List).cast<Map<String, dynamic>>();
  bool supports(String op) => (guard['operations'] as List).contains(op);
  static NativeLibraryVm? parse(Object? raw) {
    if (raw is! Map<String, dynamic> ||
        raw['label'] is! String ||
        !const ['mixer', 'simulation'].contains(raw['authority']) ||
        raw['connected'] is! bool ||
        raw['complete'] is! bool ||
        raw['capacity'] is! int ||
        (raw['capacity'] as int) < 1 ||
        raw['generation'] is! int ||
        raw['revision'] is! int ||
        raw['operations'] is! List ||
        raw['slots'] is! List ||
        raw['limitations'] is! String) {
      return null;
    }
    final ids = <int>{};
    for (final row in raw['slots'] as List) {
      if (row is! Map<String, dynamic> ||
          row['slot'] is! int ||
          row['name'] is! String ||
          !ids.add(row['slot'] as int)) {
        return null;
      }
    }
    return NativeLibraryVm._(Map.unmodifiable(raw));
  }
}

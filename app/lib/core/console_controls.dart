import 'package:flutter/foundation.dart';

/// One provider-advertised branch or control in the current console session.
enum ConsoleControlKind {
  branch,
  integer,
  number,
  boolean,
  text,
  enumeration,
  bitset,
  action,
}

class ConsoleControlChoice {
  const ConsoleControlChoice({required this.label, required this.value});
  final String label;
  final Object? value;
}

class ConsoleControlItem {
  const ConsoleControlItem({
    required this.path,
    required this.label,
    required this.kind,
    required this.session,
    required this.revision,
    this.model,
    this.value,
    this.valueKnown = false,
    this.readOnly = false,
    this.unavailableReason,
    this.unit,
    this.min,
    this.max,
    this.step,
    this.choices = const [],
  });

  final String path;
  final String label;
  final ConsoleControlKind kind;
  final Object session;
  final Object? revision;
  final Object? model;
  final Object? value;
  final bool valueKnown;
  final bool readOnly;
  final String? unavailableReason;
  final String? unit;
  final num? min;
  final num? max;
  final num? step;
  final List<ConsoleControlChoice> choices;
}

class ConsoleControlCrumb {
  const ConsoleControlCrumb({required this.label, required this.path});
  final String label;
  final String path;
}

/// The shell supplies live descriptors and guards writes against the captured
/// session, revision, model and current value; screens never access the engine.
abstract class ConsoleControlsStore extends ChangeNotifier {
  List<ConsoleControlCrumb> get breadcrumbs;
  List<ConsoleControlItem> get items;
  bool get loading;
  String? get unavailableReason;
  String? get receipt;
  void open(String path);
  void refresh();
  Future<String?> write(ConsoleControlItem captured, Object? value);
}

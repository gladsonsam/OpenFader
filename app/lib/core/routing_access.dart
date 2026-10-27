// Provider identity resolution shared by the final raw/guarded send boundary.
import 'access_limits.dart';

bool routingCommandAccessAllowed(
  Map command,
  Iterable<Map> destinations, {
  required bool Function(AccessFamily) familyAllowed,
  required bool Function(String) targetAllowed,
  required Iterable<int> Function(int) membersFor,
  required String? Function(int) memberKey,
}) {
  final cmd = command['cmd'];
  final key = command['key'];
  final destination = destinations
      .where((d) => cmd == 'route_patch'
          ? d['id'] == command['destination']
          : d['output'] != true &&
              (key is String
                  ? d['key'] == key
                  : d['strip'] == command['strip']) &&
              (d['target'] == (command['target'] ?? '') ||
                  (command['target'] ?? '') == '' && d['target'] == 'main'))
      .firstOrNull;
  if (destination == null || destination['key'] is! String) return false;
  if (!familyAllowed(destination['output'] == true
          ? AccessFamily.outputRouting
          : AccessFamily.inputRouting) ||
      !targetAllowed(destination['key'] as String)) {
    return false;
  }
  if (destination['output'] == true) return cmd == 'route_patch';
  final strip = destination['strip'];
  if (strip is! int) return false;
  return membersFor(strip).every((member) {
    final key = memberKey(member);
    return key != null && targetAllowed(key);
  });
}

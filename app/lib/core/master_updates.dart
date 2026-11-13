import '../engine.dart';

bool applyMasterFrame(EngineState state, Map<String, dynamic> frame) {
  final target = frame['target'];
  final fader = frame['fader'];
  final muted = frame['muted'];
  if (target == 'main') {
    if (fader is num && fader.isFinite) state.mainFader = fader.toDouble();
    if (muted is bool) state.mainMuted = muted;
    return true;
  }
  if (target is! String) return false;
  final match = RegExp(r'^(bus|subgroup)/([0-9]+)$').firstMatch(target);
  if (match == null) return false;
  final bus = match[1] == 'bus';
  final index = int.parse(match[2]!) - 1;
  final count = bus ? state.busCount : state.subgroupCount;
  if (index < 0 || index >= count) return false;
  if (fader is num && fader.isFinite) {
    (bus ? state.busFaders : state.subgroupFaders)[index] = fader.toDouble();
  }
  if (muted is bool) {
    (bus ? state.busMuted : state.subgroupMuted)[index] = muted;
  }
  return true;
}

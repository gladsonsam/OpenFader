// Shell adapter: routing responsibility. The shell owns polling.
import '../engine.dart';
import '../screens/routing.dart';
import 'store_adapter.dart';

class ShellRoutingStore extends ShellStoreAdapter implements RoutingStore {
  ShellRoutingStore(super.app);

  List<Map<String, dynamic>> get _groups => [
        for (final g in engine.routingCaps['groups'] as List? ?? const [])
          if (g is Map<String, dynamic>) g,
      ];
  @override
  List<RoutePatchTarget> get targets => [
        for (final t in engine.routingCaps['targets'] as List? ?? const [])
          if (t is Map && t['key'] is String && t['label'] is String)
            RoutePatchTarget(t['key'] as String, t['label'] as String,
                t['clearable'] == true),
      ];
  String _target = '';
  @override
  String get target => targets.any((t) => t.key == _target)
      ? _target
      : targets.firstOrNull?.key ?? '';
  @override
  bool get canClear =>
      targets.isEmpty || targets.any((t) => t.key == target && t.clearable);
  @override
  void selectTarget(String target) {
    if (!targets.any((t) => t.key == target)) return;
    _target = target;
    notifyListeners();
  }

  @override
  String groupLabel(String group) =>
      _groups.where((g) => g['key'] == group).firstOrNull?['label']
          as String? ??
      group;
  String _group = 'Lcl';
  int? _sel;

  @override
  List<RouteStrip> get strips => [
        for (final s in engine.strips)
          RouteStrip(
            strip: s.ch,
            id: 'in${s.ch + 1}',
            name: s.name.isEmpty ? 'CH${s.ch + 1}' : s.name,
            src: _srcOf(s.ch),
          ),
      ];

  String _sourceLabel(String group, int index) {
    if (group.isEmpty && index == 0) return '—';
    final count = _groups.where((g) => g['key'] == group).firstOrNull?['count'];
    return count == 1 ? groupLabel(group) : '${groupLabel(group)} $index';
  }

  String _srcOf(int ch) {
    for (final p in engine.routing) {
      final strip = (p['strip'] as num?)?.toInt();
      if (strip == ch && (p['target'] ?? '') == target) {
        final src = p['label'] ?? p['source'] ?? p['src'];
        if (src is String && src.isNotEmpty) {
          final group = p['group'];
          final index = p['index'];
          return group is String && index is num
              ? _sourceLabel(group, index.toInt())
              : src;
        }
      }
    }
    return '';
  }

  @override
  List<String> get groups => [for (final g in _groups) g['key'] as String];

  @override
  String get group =>
      groups.contains(_group) ? _group : groups.firstOrNull ?? '';

  @override
  int? get selectedStrip => _sel;

  @override
  String get selectedName {
    if (_sel == null) return '';
    for (final s in engine.strips) {
      if (s.ch == _sel) return s.name.isEmpty ? 'CH${s.ch + 1}' : s.name;
    }
    return 'CH${_sel! + 1}';
  }

  @override
  String get selectedSrc {
    if (_sel == null) return '';
    return _srcOf(_sel!);
  }

  @override
  List<RouteSrcTile> get srcTiles => [
        for (var i = 1;
            i <=
                ((_groups.where((g) => g['key'] == group).firstOrNull?['count']
                            as num?)
                        ?.toInt() ??
                    0);
            i++)
          RouteSrcTile(
            label: _sourceLabel(group, i),
            group: group,
            index: i,
            usedBy: _usedBy(group, i),
            current: engine.routing.any((p) =>
                p['strip'] == _sel &&
                (p['target'] ?? '') == target &&
                p['group'] == group &&
                p['index'] == i),
          ),
      ];

  String _usedBy(String group, int index) {
    for (final s in engine.strips) {
      if (engine.routing.any((p) =>
          p['strip'] == s.ch &&
          (p['target'] ?? '') == target &&
          p['group'] == group &&
          p['index'] == index)) {
        return s.name.isEmpty ? 'CH${s.ch + 1}' : s.name;
      }
    }
    return '';
  }

  // Patch commands are addressable once caps answers, even before routing is parsed.

  @override
  bool get routingSupported => engine.routingCaps['supported'] == true;

  @override
  void selectStrip(int strip) {
    _sel = strip;
    notifyListeners();
  }

  @override
  void selectGroup(String group) {
    _group = group;
    notifyListeners();
  }

  @override
  void patch(RouteSrcTile tile) {
    if (_sel == null ||
        !routingSupported ||
        !srcTiles.any((t) => t.group == tile.group && t.index == tile.index)) {
      return;
    }
    send(Cmds.patchSet(_sel!, tile.group, tile.index, target: target));
  }

  @override
  void clearPatch(int strip) {
    if (!routingSupported || !canClear) return;
    send(Cmds.patchClear(strip, target: target));
  }
}

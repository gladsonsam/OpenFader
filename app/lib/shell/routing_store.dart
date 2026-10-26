// Shell adapter: provider routing contract. The shell owns polling.
import 'dart:convert';

import '../core/access_limits.dart';
import '../screens/routing.dart';
import 'store_adapter.dart';

class ShellRoutingStore extends ShellStoreAdapter implements RoutingStore {
  ShellRoutingStore(super.app);

  List<Map> get _planes => [
        for (final p in engine.routingCaps['planes'] as List? ?? const [])
          if (p is Map && p['key'] is String) p,
      ];
  Map? get _plane => _planes.where((p) => p['key'] == target).firstOrNull;
  List<Map> get _destinations => [
        for (final d in _plane?['destinations'] as List? ?? const [])
          if (d is Map && d['id'] is String && d['strip'] is num) d,
      ];
  Map? _destination(int? strip) =>
      _destinations.where((d) => d['strip'] == strip).firstOrNull;
  Map? get _selected => _destination(_sel);
  List<Map> get _groups => [
        for (final g in (_selected ??
                _destinations.firstOrNull)?['source_groups'] as List? ??
            const [])
          if (g is Map && g['key'] is String && g['count'] is num) g,
      ];
  String _target = '';
  String _group = '';
  int? _sel;
  String? _selection;
  int _sequence = 0;
  @override
  String? get operationKey => _selection;

  @override
  List<RoutePatchTarget> get targets => [
        for (final p in _planes)
          RoutePatchTarget(p['key'] as String, p['label'] as String, false),
      ];
  @override
  String get target => targets.any((t) => t.key == _target)
      ? _target
      : targets.firstOrNull?.key ?? '';
  @override
  bool get routingSupported =>
      engine.routingCaps['supported'] == true && _planes.isNotEmpty;

  Map? _route(Map d) => engine.routing
      .where(
        (p) =>
            p['destination'] == d['id'] &&
            p['key'] == d['key'] &&
            p['strip'] == d['strip'] &&
            (p['target'] ?? '') == d['target'],
      )
      .firstOrNull;

  // Capture identity, session, access policy and current state together. A stale
  // callback cannot silently retarget after reconnect, reselect or a desk edit.
  String? _identity(Map? d) {
    if (d == null) return null;
    final route = _route(d);
    if (route == null || route['authority'] != 'provider_display') return null;
    return jsonEncode([
      _sequence,
      engine.session,
      engine.driver,
      engine.routingCaps['epoch'],
      app.controlAccessEpoch,
      d['id'],
      d['key'],
      d['strip'],
      route['group'],
      route['index'],
    ]);
  }

  @override
  bool get canPatch =>
      !isDisposed &&
      routingSupported &&
      engine.connected &&
      app.accessFamilyAllowed(AccessFamily.inputRouting) &&
      _selection != null &&
      _selection == _identity(_selected) &&
      !engine.locks.containsKey(_sel) &&
      _membersAllowed;

  bool get _membersAllowed {
    final d = _selected;
    if (d == null || !app.accessTargetAllowed(d['key'] as String)) return false;
    // Backend owns provider control_members. Conservatively protect both sides
    // while stereo state is linked or unknown, including peer local limits.
    final pair = engine.stereoPairs.where((p) => p.contains(_sel)).firstOrNull;
    if (pair == null ||
        pair.every((ch) => engine.params[ch]?['link'] == false)) {
      return true;
    }
    return pair.every((ch) {
      final key = engine.strips.where((s) => s.ch == ch).firstOrNull?.key;
      return key != null &&
          app.accessTargetAllowed(key) &&
          !engine.locks.containsKey(ch);
    });
  }

  @override
  bool get canClear => canPatch && _selected?['clearable'] == true;
  @override
  void selectTarget(String value) {
    if (!targets.any((t) => t.key == value)) return;
    _sequence++;
    _target = value;
    _sel = null;
    _selection = null;
    notifyListeners();
  }

  String _name(Map d) =>
      engine.strips
          .where((s) => s.key == d['key'] && s.ch == d['strip'])
          .map((s) => s.name)
          .where((n) => n.isNotEmpty)
          .firstOrNull ??
      d['label'] as String;

  String _sourceLabel(Map? route) {
    if (route == null) return 'Unknown';
    final group = route['group'] as String;
    final index = (route['index'] as num).toInt();
    if (group.isEmpty && index == 0) return '—';
    return _groups.where((g) => g['key'] == group).firstOrNull?['count'] == 1
        ? groupLabel(group)
        : '${groupLabel(group)} $index';
  }

  @override
  List<RouteStrip> get strips => [
        for (final d in _destinations)
          RouteStrip(
            strip: (d['strip'] as num).toInt(),
            id: d['id'] as String,
            name: _name(d),
            src: _sourceLabel(_route(d)),
          ),
      ];
  @override
  String groupLabel(String group) =>
      _groups.where((g) => g['key'] == group).firstOrNull?['label']
          as String? ??
      group;
  @override
  List<String> get groups => [for (final g in _groups) g['key'] as String];
  @override
  String get group =>
      groups.contains(_group) ? _group : groups.firstOrNull ?? '';
  @override
  int? get selectedStrip => _selected == null ? null : _sel;
  @override
  String get selectedName => _selected == null ? '' : _name(_selected!);
  @override
  String get selectedSrc =>
      _selected == null ? '' : _sourceLabel(_route(_selected!));

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
            label: _sourceLabel({'group': group, 'index': i}),
            group: group,
            index: i,
            operationKey: _identity(_selected),
            current: _selected != null &&
                _route(_selected!)?['group'] == group &&
                _route(_selected!)?['index'] == i,
            usedBy: _destinations
                    .where(
                      (d) =>
                          _route(d)?['group'] == group &&
                          _route(d)?['index'] == i,
                    )
                    .map(_name)
                    .firstOrNull ??
                '',
          ),
      ];
  @override
  void selectStrip(int strip) {
    final d = _destination(strip);
    if (d == null) return;
    _sequence++;
    _sel = strip;
    _selection = _identity(d);
    notifyListeners();
  }

  @override
  void selectGroup(String value) {
    if (!groups.contains(value)) return;
    _group = value;
    notifyListeners();
  }

  void _write(Map<String, Object>? source) {
    final d = _selected;
    if (!canPatch || d == null || (source == null && !canClear)) return;
    send(
      jsonEncode({
        'cmd': 'route_patch',
        'driver': engine.driver,
        'epoch': engine.routingCaps['epoch'],
        'destination': d['id'],
        'expected': _route(d)!['label'],
        'source': source,
      }),
    );
    // Require a fresh selection before another operation, without optimistic
    // authority or treating an unrelated event as an acknowledgement.
    _selection = null;
    notifyListeners();
  }

  @override
  void patch(RouteSrcTile tile) {
    if (tile.operationKey == null ||
        tile.operationKey != _selection ||
        !srcTiles.any(
          (t) =>
              t.group == tile.group &&
              t.index == tile.index &&
              t.operationKey == tile.operationKey,
        )) {
      return;
    }
    _write({'group': tile.group, 'index': tile.index});
  }

  @override
  void clearPatch(int strip, {String? operationKey}) {
    if (strip != _sel || operationKey == null || operationKey != _selection) {
      return;
    }
    _write(null);
  }
}

// Shell adapter: provider routing contract. The shell owns polling.
import 'dart:convert';

import '../core/access_limits.dart';
import '../core/destination_controls.dart';
import '../screens/routing.dart';
import 'store_adapter.dart';

class ShellRoutingStore extends ShellStoreAdapter implements RoutingStore {
  ShellRoutingStore(super.app);

  List _list(Object? value) => value is List ? value : const [];

  List<Map> get _planes => [
        for (final p in _list(engine.routingCaps['planes']))
          if (p is Map && p['key'] is String && p['label'] is String) p,
      ];
  Map? get _plane => _planes.where((p) => p['key'] == target).firstOrNull;
  List<Map> get _destinations => [
        for (final d in _list(_plane?['destinations']))
          if (d is Map &&
              d['id'] is String &&
              d['key'] is String &&
              d['label'] is String &&
              d['target'] is String &&
              d['strip'] is int &&
              (d['strip'] as int) >= 0)
            d,
      ];
  Map? _destination(int? strip) =>
      _destinations.where((d) => d['strip'] == strip).firstOrNull;
  Map? get _selected => _destination(_sel);
  List<Map> get _groups => [
        for (final g in _list(
            (_selected ?? _destinations.firstOrNull)?['source_groups']))
          if (g is Map &&
              g['key'] is String &&
              g['label'] is String &&
              g['count'] is int &&
              (g['count'] as int) > 0 &&
              (g['count'] as int) <= 255)
            g,
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

  @override
  int get controlAccessEpoch => app.controlAccessEpoch;

  @override
  DestinationControlVm? get tapControl {
    final id = _selected?['id'];
    return engine.destinationControls
        .where((control) =>
            control.target == id &&
            control.surface == 'routing' &&
            control.id == 'tap')
        .firstOrNull;
  }

  @override
  bool tapIsCurrent(DestinationControlVm control) =>
      canPatch &&
      control.target == _selected?['id'] &&
      app.destinationControlIsCurrent(control);

  @override
  Future<String?> setTapControl(DestinationControlVm control, double value) =>
      tapIsCurrent(control)
          ? app.setDestinationControl(control, value)
          : Future.value('Routing changed; reopen the control.');

  Map? _route(Map d) {
    final route = engine.routing
        .where((p) =>
            p['destination'] == d['id'] &&
            p['key'] == d['key'] &&
            p['strip'] == d['strip'] &&
            (p['target'] ?? '') == d['target'])
        .firstOrNull;
    if (route == null ||
        route['group'] is! String ||
        route['index'] is! int ||
        route['label'] is! String) {
      return null;
    }
    final group = route['group'] as String;
    final index = route['index'] as int;
    if (group.isEmpty && index == 0 && route['label'] == '—') return route;
    final allowed = _list(d['source_groups']).whereType<Map>().any((g) =>
        g['key'] == group &&
        g['count'] is int &&
        index > 0 &&
        index <= (g['count'] as int));
    // Display and command expectation must describe the same source socket.
    final parts = (route['label'] as String).split(' ');
    if (!allowed ||
        parts.length != 2 ||
        parts.first != group ||
        int.tryParse(parts.last) != index) {
      return null;
    }
    return route;
  }

  // Capture identity, session, access policy and current state together. A stale
  // callback cannot silently retarget after reconnect, reselect or a desk edit.
  String? _identity(Map? d) {
    if (d == null || engine.routingCaps['epoch'] is! int) return null;
    final route = _route(d);
    if (route == null ||
        (d['output'] == true
            ? route['authority'] != 'verified' || route['guard'] is! Map
            : route['authority'] != 'provider_display')) {
      return null;
    }
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
      route['guard'],
    ]);
  }

  @override
  bool get canPatch =>
      !isDisposed &&
      routingSupported &&
      engine.connected &&
      app.accessFamilyAllowed(
        _selected?['output'] == true
            ? AccessFamily.outputRouting
            : AccessFamily.inputRouting,
      ) &&
      _selection != null &&
      _selection == _identity(_selected) &&
      (_selected?['output'] == true || !engine.locks.containsKey(_sel)) &&
      _membersAllowed;

  bool get _membersAllowed {
    final d = _selected;
    if (d == null || !app.accessTargetAllowed(d['key'] as String)) return false;
    if (d['output'] == true) return true;
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
            displayGroupLabel:
                _groups.where((g) => g['key'] == group).firstOrNull?['count'] ==
                        1
                    ? null
                    : groupLabel(group),
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
        if (_route(d)!['guard'] != null) 'guard': _route(d)!['guard'],
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

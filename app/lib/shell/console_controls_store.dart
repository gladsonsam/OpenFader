import 'dart:async';

import '../core/access_limits.dart';
import '../core/console_controls.dart';
import '../engine.dart';
import 'store_adapter.dart';

class ShellConsoleControlsStore extends ShellStoreAdapter
    implements ConsoleControlsStore {
  ShellConsoleControlsStore(super.app);

  final _clock = Stopwatch()..start();
  final _prefix = DateTime.now().microsecondsSinceEpoch;
  final Map<String, Completer<String?>> _writes = {};
  List<ConsoleControlItem> _items = const [];
  Object? _session;
  int _proofRevision = -1;
  String _path = '/', _label = 'Console controls';
  String? _readId, _reason, _receipt;
  int _serial = 0, _readStarted = 0, _lastRead = -1000;

  Object get _currentSession =>
      (engine.driver, engine.session, engine.syncEpoch);
  Object get _capturedSession => (_currentSession, engine.nativeProofRevision);
  bool get _editable =>
      engine.connected &&
      engine.capsReady &&
      AccessFamily.values.every(app.accessFamilyAllowed) &&
      engine.strips.every((s) => app.accessTargetAllowed(s.key)) &&
      engine.locks.values.every((owner) => owner == engine.clientId);

  @override
  List<ConsoleControlItem> get items => _items;
  @override
  List<ConsoleControlCrumb> get breadcrumbs {
    final parts = _path.split('/').where((p) => p.isNotEmpty).toList();
    return [
      const ConsoleControlCrumb(label: 'Console', path: '/'),
      for (var n = 0; n < parts.length; n++)
        ConsoleControlCrumb(
          label: n == parts.length - 1 ? _label : parts[n],
          path: '/${parts.take(n + 1).join('/')}',
        ),
    ];
  }

  @override
  bool get loading => _readId != null && _items.isEmpty;
  @override
  String? get unavailableReason => !engine.connected
      ? 'Connect to a mixer to read its console controls.'
      : _reason;
  @override
  String? get receipt => _receipt;

  void _sync() {
    if (_session != _currentSession) {
      _session = _currentSession;
      _items = const [];
      _path = '/';
      _label = 'Console controls';
      _readId = null;
      _reason = null;
      _receipt = null;
      _lastRead = -1000;
      for (final write in _writes.values) {
        if (!write.isCompleted) {
          write.complete('Connection changed; inspect the control again.');
        }
      }
      _writes.clear();
    }
    if (_proofRevision != engine.nativeProofRevision) {
      _proofRevision = engine.nativeProofRevision;
      _items = const [];
      _readId = null;
      _lastRead = -1000;
    }
    final response = engine.consoleControlNodes.remove(_readId);
    if (response != null) {
      _readId = null;
      final node = response['node'];
      if (node is Map && node['path'] == _path && node['items'] is List) {
        _label = node['label'] as String? ?? _label;
        _reason =
            response['error'] as String? ??
            node['unavailable_reason'] as String?;
        _items = (node['items'] as List)
            .whereType<Map>()
            .map((row) => _item(row))
            .toList(growable: false);
      } else {
        _reason = 'The provider returned an invalid console descriptor.';
        _items = const [];
      }
    }
    for (final id in _writes.keys.toList()) {
      final result = engine.consoleControlResults.remove(id);
      if (result == null) continue;
      final status = result['status'];
      _receipt =
          result['message'] as String? ??
          switch (status) {
            'verified' => 'Setting verified on the mixer.',
            'accepted' => 'Command accepted; resulting state is not verified.',
            'uncertain' => 'The outcome is uncertain. Refresh before retrying.',
            _ => 'The mixer rejected this change.',
          };
      final control = result['control'];
      if (status == 'verified' && control is Map) {
        final updated = _item(control);
        _items = [
          for (final row in _items) row.path == updated.path ? updated : row,
        ];
      }
      _lastRead = -1000;
      final pending = _writes.remove(id)!;
      if (!pending.isCompleted) pending.complete(_receipt);
    }
  }

  ConsoleControlItem _item(Map raw) {
    final kind = ConsoleControlKind.values
        .where((k) => k.name == raw['kind'])
        .firstOrNull;
    final choices = raw['choices'];
    final valueKnown = raw.containsKey('value');
    final restriction = !_editable
        ? 'Console edits require unrestricted mixer access.'
        : null;
    final reason =
        raw['unavailable_reason'] as String? ??
        restriction ??
        (kind == null ? 'This control type is unavailable.' : null);
    return ConsoleControlItem(
      path: raw['path'] as String? ?? '',
      label: raw['label'] as String? ?? '',
      kind: kind ?? ConsoleControlKind.text,
      session: _capturedSession,
      revision: raw['revision'],
      model: raw['model'],
      value: raw['value'],
      valueKnown: valueKnown,
      readOnly:
          raw['read_only'] != false ||
          reason != null ||
          raw['revision'] == null,
      unavailableReason: reason,
      unit: raw['unit'] as String?,
      min: raw['min'] as num?,
      max: raw['max'] as num?,
      step: raw['step'] as num?,
      choices: choices is List
          ? [
              for (final choice in choices.whereType<Map>())
                if (choice['label'] is String && choice.containsKey('value'))
                  ConsoleControlChoice(
                    label: choice['label'] as String,
                    value: choice['value'],
                  ),
            ]
          : const [],
    );
  }

  @override
  void forwardChange() {
    _sync();
    super.forwardChange();
  }

  @override
  void open(String path) {
    _sync();
    if (!path.startsWith('/') || path.length > 512) return;
    _path = path;
    _items = const [];
    _reason = null;
    _readId = null;
    refresh();
  }

  @override
  void refresh() {
    _sync();
    if (isDisposed || !engine.connected || _readId != null) return;
    _readId = 'console-$_prefix-${++_serial}';
    _readStarted = _lastRead = _clock.elapsedMilliseconds;
    send(Cmds.consoleControls(_readId!, _path));
    notifyListeners();
  }

  void poll() {
    if (isDisposed) return;
    _sync();
    final now = _clock.elapsedMilliseconds;
    if (_readId != null && now - _readStarted > 15000) {
      _readId = null;
      _reason = 'Console controls could not be refreshed.';
      notifyListeners();
    }
    if (_writes.isEmpty && now - _lastRead >= 1000) refresh();
  }

  @override
  Future<String?> write(ConsoleControlItem captured, Object? value) async {
    _sync();
    final epoch = engine.syncEpoch;
    if (isDisposed ||
        !_editable ||
        epoch == null ||
        captured.session != _capturedSession ||
        captured.readOnly ||
        captured.revision == null ||
        _writes.isNotEmpty) {
      return 'Connection, access or control context changed. Refresh before editing.';
    }
    final id = 'console-$_prefix-${++_serial}';
    final pending = Completer<String?>();
    _writes[id] = pending;
    send(
      Cmds.consoleControlSet(
        id,
        epoch,
        captured.path,
        value,
        captured.revision!,
      ),
    );
    try {
      return await pending.future.timeout(const Duration(seconds: 30));
    } on TimeoutException {
      _writes.remove(id);
      _receipt = 'No mixer confirmation; the outcome is uncertain. Refresh before retrying.';
      notifyListeners();
      return _receipt;
    }
  }

  @override
  void dispose() {
    for (final write in _writes.values) {
      if (!write.isCompleted) {
        write.complete(
          'Console controls closed; inspect the mixer before retrying.',
        );
      }
    }
    _writes.clear();
    _clock.stop();
    super.dispose();
  }
}

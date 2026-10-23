// W2a param store: caps index + confirmed values + optimistic pending edits
// (optimistic set, confirm-into-pending, timeout revert). Pure Dart: no
// Flutter, no transport.
//
// Confirmation epsilon covers the desk quantization quantum (~2e-3,
// 10-bit fader steps): a confirmed echo must clear its optimistic edit.
library;

/// Wire param value: string | bool | finite number (mirrors TS ParamValue).
typedef ParamValue = Object;

/// One indexed caps row (ranges in engineering units).
class CapsRow {
  final String id;
  final String kind;
  final double min;
  final double max;
  final String unit;
  final ParamValue def;
  final String label;
  final List<String> scopes;

  const CapsRow({
    required this.id,
    required this.kind,
    required this.min,
    required this.max,
    required this.unit,
    required this.def,
    required this.label,
    this.scopes = const [],
  });
}

/// Indexed capabilities: `ready` is false until the first caps frame lands.
class CapsSnapshot {
  final bool ready;
  final String driver;
  final Map<String, CapsRow> defs;
  final List<String> supported;
  final List<String> unsupported;

  const CapsSnapshot({
    required this.ready,
    required this.driver,
    required this.defs,
    required this.supported,
    required this.unsupported,
  });
}

/// Optimistic-write confirmation window before a revert.
const int confirmTimeoutMs = 2000;

/// Focus sends coalesce rapid scroll/layer changes into one frame.
const int focusDebounceMs = 150;

CapsSnapshot emptyCaps() => const CapsSnapshot(
      ready: false,
      driver: '',
      defs: {},
      supported: [],
      unsupported: [],
    );

double _numOf(Object? v) => v is num && v.isFinite ? v.toDouble() : 0;

ParamValue _defOf(Object? raw) {
  if (raw is String) return raw;
  if (raw is bool) return raw;
  if (raw is num && raw.isFinite) return raw;
  return 0;
}

/// Index a caps frame. Unknown/malformed entries are skipped.
CapsSnapshot capsFromInput(Map<String, dynamic> input) {
  final defs = <String, CapsRow>{};
  final supported = <String>[];
  final params = input['params'];
  if (params is List) {
    for (final p in params) {
      if (p is! Map) continue;
      final id = p['id'];
      if (id is! String || id.isEmpty) continue;
      if (defs.containsKey(id)) continue;
      final scopesRaw = p['scopes'];
      defs[id] = CapsRow(
        id: id,
        kind: p['kind'] is String ? p['kind'] as String : '',
        min: _numOf(p['min']),
        max: _numOf(p['max']),
        unit: p['unit'] is String ? p['unit'] as String : '',
        def: _defOf(p['default']),
        label: p['label'] is String ? p['label'] as String : id,
        scopes: scopesRaw is List
            ? [for (final s in scopesRaw) if (s is String) s]
            : const [],
      );
      supported.add(id);
    }
  }
  final unsupportedRaw = input['unsupported'];
  return CapsSnapshot(
    ready: true,
    driver: input['driver'] is String ? input['driver'] as String : '',
    defs: defs,
    supported: supported,
    unsupported: unsupportedRaw is List
        ? [for (final s in unsupportedRaw) if (s is String && s.isNotEmpty) s]
        : const [],
  );
}

/// Confirmed per-channel values (channel -> id -> engineering value).
typedef ChannelValues = Map<int, Map<String, ParamValue>>;

class PendingEdit {
  final ParamValue value;
  final double atMs;
  final bool held;

  const PendingEdit({required this.value, required this.atMs, this.held = false});
}

/// Optimistic edits awaiting confirmation, keyed `${ch}:${id}`.
typedef PendingMap = Map<String, PendingEdit>;

class StoreMaps {
  final ChannelValues confirmed = {};
  final PendingMap pending = {};
}

StoreMaps emptyMaps() => StoreMaps();

String pendingKey(int ch, String id) => '$ch:$id';

/// True for relay-usable values (non-finite numbers are never confirmed).
bool validValue(Object? v) =>
    v is String || v is bool || (v is num && v.isFinite);

/// Merge a `params` diff/snapshot into the confirmed map. Invalid entries
/// are skipped. Returns true when anything actually changed.
bool applyParams(StoreMaps maps, int ch, Map<String, ParamValue> values) {
  if (ch < 0) return false;
  var changed = false;
  for (final e in values.entries) {
    if (e.key.isEmpty || !validValue(e.value)) continue;
    final slot = maps.confirmed.putIfAbsent(ch, () => {});
    if (!valuesEqual(slot[e.key], e.value)) {
      slot[e.key] = e.value;
      changed = true;
    }
    confirmInto(maps.pending, pendingKey(ch, e.key), e.value);
  }
  return changed;
}

/// Numbers compare within the desk's quantization step; other types compare
/// by identity.
bool valuesEqual(ParamValue? a, ParamValue b) {
  if (a is num && b is num) return (a - b).abs() < 2e-3;
  return a == b;
}

/// Drop a pending edit when the relay confirms the same value.
bool confirmInto(PendingMap pending, String key, ParamValue value) {
  final edit = pending[key];
  if (edit == null) return false;
  if (!edit.held && valuesEqual(edit.value, value)) {
    pending.remove(key);
    return true;
  }
  return false;
}

/// Record an optimistic edit. The display shows the pending value at once.
String noteOptimistic(
    StoreMaps maps, int ch, String id, ParamValue value, double nowMs) {
  final key = pendingKey(ch, id);
  maps.pending[key] = PendingEdit(value: value, atMs: nowMs);
  return key;
}

/// Drop pending edits older than [timeoutMs]; returns the expired keys.
List<String> sweepPending(PendingMap pending, double nowMs, int timeoutMs) {
  final expired = <String>[];
  for (final e in pending.entries.toList()) {
    if (nowMs - e.value.atMs >= timeoutMs) {
      pending.remove(e.key);
      expired.add(e.key);
    }
  }
  return expired;
}

/// Split a pending key back into channel + id (null when malformed).
({int ch, String id})? splitKey(String key) {
  final i = key.indexOf(':');
  if (i <= 0) return null;
  final ch = int.tryParse(key.substring(0, i));
  final id = key.substring(i + 1);
  if (ch == null || ch < 0 || id.isEmpty) return null;
  return (ch: ch, id: id);
}

/// Display value: pending optimistic edit wins, else the confirmed value.
ParamValue? optimisticValue(StoreMaps maps, int ch, String id) =>
    maps.pending[pendingKey(ch, id)]?.value ?? maps.confirmed[ch]?[id];

/// True when the channel/id pair holds an unconfirmed optimistic edit.
bool isPending(StoreMaps maps, int ch, String id) =>
    maps.pending.containsKey(pendingKey(ch, id));

/// True when the relay has confirmed any value for the channel/id pair.
bool confirmedKnown(StoreMaps maps, int ch, String id) =>
    maps.confirmed[ch]?.containsKey(id) ?? false;

/// Clamp/round a write into the caps row: floats clamp into range, enum-ish
/// numbers round, unknown ids pass through untouched.
ParamValue coerceWrite(Map<String, CapsRow> defs, String id, ParamValue v) {
  final row = defs[id];
  if (row == null) return v;
  if (v is num && v.isFinite) {
    final lo = row.min;
    final hi = row.max;
    if (row.kind == 'enum' || row.kind == 'int') {
      return v.round().clamp(lo.toInt(), hi.toInt());
    }
    if (row.kind == 'float' || row.kind == '') {
      return v.toDouble().clamp(lo, hi);
    }
  }
  return v;
}

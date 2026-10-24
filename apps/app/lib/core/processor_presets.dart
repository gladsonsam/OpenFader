// Shared processor (EQ/gate/compressor) preset contracts + catalogue.
//
// Pure Dart (only `Listenable` from foundation for the store contract).
// No SharedPreferences, no timers, no hardware here: persistence lives in
// `lib/settings/processor_presets.dart` (`ProcessorPresetLibrary`), and the
// main `AppStore` implements `ProcessorPresetStore` over the engine + that
// library. Settings `fullReset` clears the stored document via the library
// (owned by the parent, not here).
//
// Design notes for the parent (Sol UI / state owner):
// - Presets are per-device, per `driver` + `family` + `kind`. Raw `values`
//   stay provider-specific (enum encodings differ per desk); the
//   driver/family/kind scope stops cross-driver enum reuse.
// - Bypass/on flags are never preset fields (any `*_on` / `eq_on`). Recall
//   must leave bypass states alone; only the allow-listed DSP ids below are
//   captured and recalled, filtered again by the live caps at recall time.
// - No factory/built-in presets are invented here; an empty device library
//   decodes to `[]`.
library;

import 'dart:convert';

import 'package:flutter/foundation.dart';

/// DSP block a processor preset captures.
enum ProcessorPresetKind {
  eq,
  gate,
  compressor;

  /// UI label (`EQ`, `Gate`, `Compressor`).
  String get label => switch (this) {
        ProcessorPresetKind.eq => 'EQ',
        ProcessorPresetKind.gate => 'Gate',
        ProcessorPresetKind.compressor => 'Compressor',
      };

  /// Stable wire name (`name`); used in stored JSON.
  String get wire => name;

  /// Parse a stored wire name; throws [FormatException] when unknown.
  static ProcessorPresetKind fromWire(String wire) {
    for (final kind in ProcessorPresetKind.values) {
      if (kind.name == wire) return kind;
    }
    throw FormatException('Unknown processor preset kind "$wire".');
  }
}

/// Where a preset operation applies. Built by the state owner from the
/// active strip/connection; the device library itself is keyed only by the
/// preset's `driver` + `family` + `kind` + `name`.
class ProcessorPresetTarget {
  /// Opaque session index supplied by the owner (connection/session slot).
  final int session;

  /// Channel index the operation targets.
  final int channel;

  /// Strip key (for example `ch/01`).
  final String key;

  /// Engine driver id (for example `dl32s`, `xair`, `sim`).
  final String driver;

  /// Strip family (for example `in`, `bus`, `main`); scopes enum reuse.
  final String family;

  /// Display channel name at capture time (diagnostics only).
  final String name;

  /// DSP block this target addresses.
  final ProcessorPresetKind kind;

  const ProcessorPresetTarget({
    required this.session,
    required this.channel,
    required this.key,
    required this.driver,
    required this.family,
    required this.name,
    required this.kind,
  });
}

/// One stored device preset. Immutable; [values] is an unmodifiable copy.
class ProcessorPreset {
  /// Stable unique id (owner-generated; upsert key in the library).
  final String id;

  /// Local display name (trimmed, Unicode-preserving; see
  /// [validateProcessorPresetName]).
  final String name;

  /// Engine driver id this preset was captured from.
  final String driver;

  /// Strip family this preset applies to.
  final String family;

  /// DSP block this preset holds.
  final ProcessorPresetKind kind;

  /// Canonical DSP id -> finite primitive (`String | bool | num`).
  /// Keys always satisfy [processorPresetField] for [kind].
  final Map<String, Object> values;

  final DateTime createdAt;
  final DateTime updatedAt;

  ProcessorPreset({
    required this.id,
    required this.name,
    required this.driver,
    required this.family,
    required this.kind,
    required Map<String, Object> values,
    required this.createdAt,
    required this.updatedAt,
  }) : values = Map<String, Object>.unmodifiable(
          Map<String, Object>.of(values),
        );

  /// Copy with a new display name / timestamps (used by rename).
  ProcessorPreset copyWith({
    String? name,
    DateTime? updatedAt,
  }) {
    return ProcessorPreset(
      id: id,
      name: name ?? this.name,
      driver: driver,
      family: family,
      kind: kind,
      values: values,
      createdAt: createdAt,
      updatedAt: updatedAt ?? this.updatedAt,
    );
  }

  /// Strict decode; throws [FormatException] on any unknown field, bad
  /// primitive, bad scope, bad kind, or bad timestamp.
  factory ProcessorPreset.fromJson(Map<String, dynamic> json) {
    const keys = {
      'id',
      'name',
      'driver',
      'family',
      'kind',
      'values',
      'createdAt',
      'updatedAt',
    };
    if (json.keys.any((key) => !keys.contains(key))) {
      throw const FormatException('Preset contains unknown properties.');
    }
    final id = json['id'];
    final name = json['name'];
    final driver = json['driver'];
    final family = json['family'];
    final kindRaw = json['kind'];
    final valuesRaw = json['values'];
    final createdRaw = json['createdAt'];
    final updatedRaw = json['updatedAt'];
    if (id is! String || !_isToken(id, kProcessorPresetMaxIdLength)) {
      throw const FormatException('Preset has a bad id.');
    }
    if (name is! String || validateProcessorPresetName(name) != null) {
      throw const FormatException('Preset has a bad name.');
    }
    if (driver is! String ||
        !_isToken(driver, kProcessorPresetMaxScopeLength)) {
      throw const FormatException('Preset has a bad driver.');
    }
    if (family is! String ||
        !_isToken(family, kProcessorPresetMaxScopeLength)) {
      throw const FormatException('Preset has a bad family.');
    }
    if (kindRaw is! String) {
      throw const FormatException('Preset has a bad kind.');
    }
    final ProcessorPresetKind kind;
    try {
      kind = ProcessorPresetKind.fromWire(kindRaw);
    } on FormatException {
      throw const FormatException('Preset has a bad kind.');
    }
    if (valuesRaw is! Map) {
      throw const FormatException('Preset has bad values.');
    }
    if (valuesRaw.isEmpty ||
        valuesRaw.length > kProcessorPresetMaxValuesPerPreset) {
      throw const FormatException('Preset values out of bounds.');
    }
    final values = <String, Object>{};
    for (final entry in valuesRaw.entries) {
      final field = entry.key;
      final value = entry.value;
      if (field is! String || !processorPresetField(kind, field)) {
        throw FormatException('Preset has unknown field "$field".');
      }
      if (!_isPresetValue(value)) {
        throw FormatException('Preset field "$field" has a bad value.');
      }
      values[field] = value as Object;
    }
    final createdAt =
        createdRaw is String ? DateTime.tryParse(createdRaw) : null;
    final updatedAt =
        updatedRaw is String ? DateTime.tryParse(updatedRaw) : null;
    if (createdAt == null || updatedAt == null) {
      throw const FormatException('Preset has bad timestamps.');
    }
    return ProcessorPreset(
      id: id,
      name: name.trim(),
      driver: driver.trim(),
      family: family.trim(),
      kind: kind,
      values: values,
      createdAt: createdAt,
      updatedAt: updatedAt,
    );
  }

  /// Encode for the versioned device document.
  Map<String, dynamic> toJson() => {
        'id': id,
        'name': name,
        'driver': driver,
        'family': family,
        'kind': kind.wire,
        'values': Map<String, dynamic>.of(values),
        'createdAt': createdAt.toIso8601String(),
        'updatedAt': updatedAt.toIso8601String(),
      };
}

/// Recall outcome for the UI. `ok` is true when [error] is null.
class ProcessorPresetResult {
  final String? error;
  final int confirmed;
  final int total;

  const ProcessorPresetResult({
    required this.error,
    required this.confirmed,
    required this.total,
  });

  bool get ok => error == null;
}

/// Store contract the processor UI renders. Implemented by the main
/// `AppStore` over the engine (capture/recall) + `ProcessorPresetLibrary`
/// (device persistence). `String?` returns are user-facing errors (null =
/// success); recall reports per-param confirmation counts instead.
abstract class ProcessorPresetStore implements Listenable {
  /// Capture target for [kind] from the active strip, or null when the
  /// current selection cannot hold that block.
  ProcessorPresetTarget? capturePresetTarget(ProcessorPresetKind kind);

  /// Validate a target before use (null = usable). Checks driver/family
  /// scoping, not live caps; recall filters by live caps.
  String? presetTargetError(ProcessorPresetTarget target);

  /// Presets for the target scope (`driver` + `family` + `kind`), newest
  /// last (library order).
  Future<List<ProcessorPreset>> loadProcessorPresets(
    ProcessorPresetTarget target,
  );

  /// Capture the live block at [target] into the device library under
  /// [name]. Returns a user-facing error, or null on success.
  Future<String?> saveProcessorPreset(
    ProcessorPresetTarget target,
    String name,
  );

  /// Recall preset [id] onto [target]; only live-capped fields are written.
  Future<ProcessorPresetResult> recallProcessorPreset(
    ProcessorPresetTarget target,
    String id,
  );

  /// Rename preset [id] in scope. Returns a user-facing error, or null.
  Future<String?> renameProcessorPreset(
    ProcessorPresetTarget target,
    String id,
    String name,
  );

  /// Delete preset [id] in scope. Returns a user-facing error, or null.
  Future<String?> deleteProcessorPreset(
    ProcessorPresetTarget target,
    String id,
  );
}

// ---------------------------------------------------------------------------
// Catalogue: explicit allow-lists (never an arbitrary prefix).
// ---------------------------------------------------------------------------

/// SharedPreferences key for the versioned device document.
const String kProcessorPresetStorageKey = 'openfader.processorPresets';

/// Version of the stored JSON document (`{version, presets}`).
const int kProcessorPresetDocVersion = 1;

/// Bounds for the stored document (never silently grow/truncate).
const int kProcessorPresetMaxPresets = 500;
const int kProcessorPresetMaxValuesPerPreset = 64;
const int kProcessorPresetMaxDocBytes = 262144; // 256 KiB.

/// Display-name bound (shared by save/rename/decode).
const int kProcessorPresetMaxNameLength = 64;

/// Id/scope bounds.
const int kProcessorPresetMaxIdLength = 128;
const int kProcessorPresetMaxScopeLength = 64;

/// Stored string-value bound (DSP values are numbers/bools; text stays tiny).
const int kProcessorPresetMaxStringValueLength = 256;

/// EQ block: band freq/gain/q/type (1..6; recall narrows by live caps) plus
/// HPF freq/slope. `eq_on` / `hpf_on` are never preset fields.
const Set<String> _eqPresetFields = {
  'eq_mode',
  'hpf_freq',
  'hpf_slope',
  'eq1_freq',
  'eq1_gain',
  'eq1_q',
  'eq1_type',
  'eq2_freq',
  'eq2_gain',
  'eq2_q',
  'eq2_type',
  'eq3_freq',
  'eq3_gain',
  'eq3_q',
  'eq3_type',
  'eq4_freq',
  'eq4_gain',
  'eq4_q',
  'eq4_type',
  'eq5_freq',
  'eq5_gain',
  'eq5_q',
  'eq5_type',
  'eq6_freq',
  'eq6_gain',
  'eq6_q',
  'eq6_type',
};

/// Gate block: timing + mode + key + filter voice. `gate_on` and
/// `gate_filter_on` are never preset fields.
const Set<String> _gatePresetFields = {
  'gate_thr',
  'gate_range',
  'gate_att',
  'gate_hold',
  'gate_rel',
  'gate_mode',
  'gate_key_src',
  'gate_filter_type',
  'gate_filter_freq',
};

/// Compressor block: timing + modes + detector + key + filter voice + mix.
/// `comp_on` and `comp_filter_on` are never preset fields; `comp_auto` is a
/// retained timing mode, not a bypass.
const Set<String> _compPresetFields = {
  'comp_thr',
  'comp_ratio',
  'comp_knee',
  'comp_makeup',
  'comp_att',
  'comp_hold',
  'comp_rel',
  'comp_mode',
  'comp_det',
  'comp_env',
  'comp_mix',
  'comp_auto',
  'comp_key_src',
  'comp_filter_type',
  'comp_filter_freq',
};

/// True when [field] is a storable preset field for [kind].
///
/// Explicit allow-list over the canonical catalogue
/// (`crates/core/src/params.rs`); anything else — including every bypass/on
/// flag (`eq_on`, `hpf_on`, `gate_on`, `gate_filter_on`, `comp_on`,
/// `comp_filter_on`) — returns false.
bool processorPresetField(ProcessorPresetKind kind, String field) {
  return switch (kind) {
    ProcessorPresetKind.eq => _eqPresetFields.contains(field),
    ProcessorPresetKind.gate => _gatePresetFields.contains(field),
    ProcessorPresetKind.compressor => _compPresetFields.contains(field),
  };
}

/// Storable fields for [kind] (unmodifiable; same set as
/// [processorPresetField]).
Set<String> processorPresetFields(ProcessorPresetKind kind) {
  return switch (kind) {
    ProcessorPresetKind.eq => _eqPresetFields,
    ProcessorPresetKind.gate => _gatePresetFields,
    ProcessorPresetKind.compressor => _compPresetFields,
  };
}

/// True for storable primitives: `String | bool | finite num`.
bool _isPresetValue(Object? value) {
  if (value is bool) return true;
  if (value is num) return value.isFinite;
  if (value is String) {
    return value.length <= kProcessorPresetMaxStringValueLength &&
        !value.runes.any((r) => r >= 0xd800 && r <= 0xdfff);
  }
  return false;
}

bool _isToken(String value, int maxLength) {
  final trimmed = value.trim();
  if (value != trimmed || trimmed.isEmpty || trimmed.length > maxLength) {
    return false;
  }
  for (final unit in trimmed.runes) {
    if (unit < 0x20 ||
        unit == 0x7f ||
        (unit >= 0x80 && unit <= 0x9f) ||
        (unit >= 0xd800 && unit <= 0xdfff)) {
      return false;
    }
  }
  return true;
}

/// Shared display-name validation. Trims, requires non-empty, enforces the
/// length bound, and rejects control characters. Unicode names are otherwise
/// preserved verbatim. Returns a user-facing error, or null when valid.
String? validateProcessorPresetName(String name) {
  final trimmed = name.trim();
  if (trimmed.isEmpty) return 'Enter a preset name.';
  if (trimmed.length > kProcessorPresetMaxNameLength) {
    return 'Preset names can be at most $kProcessorPresetMaxNameLength characters.';
  }
  for (final unit in trimmed.runes) {
    if (unit < 0x20 ||
        unit == 0x7f ||
        (unit >= 0x80 && unit <= 0x9f) ||
        (unit >= 0xd800 && unit <= 0xdfff)) {
      return 'Preset names cannot contain control characters or invalid Unicode.';
    }
  }
  return null;
}

/// Encode presets as the bounded versioned document. Throws [StateError] on
/// bound violations (too many presets, duplicate ids, oversized document).
String encodeProcessorPresets(List<ProcessorPreset> presets) {
  if (presets.length > kProcessorPresetMaxPresets) {
    throw StateError(
      'Too many processor presets (max $kProcessorPresetMaxPresets).',
    );
  }
  final seen = <String>{};
  for (final preset in presets) {
    if (!seen.add(preset.id)) {
      throw StateError('Duplicate processor preset id.');
    }
  }
  final raw = jsonEncode({
    'version': kProcessorPresetDocVersion,
    'presets': [for (final preset in presets) preset.toJson()],
  });
  if (utf8.encode(raw).length > kProcessorPresetMaxDocBytes) {
    throw StateError('Processor preset library is too large.');
  }
  return raw;
}

/// Strict decode of the versioned document. Returns `[]` only when nothing
/// was ever stored (`null`/empty). Any corruption — bad JSON, wrong version,
/// bad shape, unknown field, bad primitive, duplicate id, over bounds —
/// throws [FormatException]; callers must surface it, never silently reset.
List<ProcessorPreset> decodeProcessorPresets(String? raw) {
  if (raw == null || raw.isEmpty) return <ProcessorPreset>[];
  if (utf8.encode(raw).length > kProcessorPresetMaxDocBytes) {
    throw const FormatException('Processor preset library is too large.');
  }
  dynamic decoded;
  try {
    decoded = jsonDecode(raw);
  } catch (_) {
    throw const FormatException('Processor presets are not valid JSON.');
  }
  if (decoded is! Map<String, dynamic>) {
    throw const FormatException('Processor presets have a bad document.');
  }
  if (decoded.keys.any((key) => key != 'version' && key != 'presets')) {
    throw const FormatException(
        'Processor presets contain unknown properties.');
  }
  if (decoded['version'] is! int ||
      decoded['version'] != kProcessorPresetDocVersion) {
    throw const FormatException('Processor presets have a bad version.');
  }
  final list = decoded['presets'];
  if (list is! List) {
    throw const FormatException('Processor presets have a bad list.');
  }
  if (list.length > kProcessorPresetMaxPresets) {
    throw const FormatException('Too many processor presets.');
  }
  final out = <ProcessorPreset>[];
  final seen = <String>{};
  for (final entry in list) {
    if (entry is! Map<String, dynamic>) {
      throw const FormatException('Processor preset has a bad entry.');
    }
    final preset = ProcessorPreset.fromJson(entry);
    if (!seen.add(preset.id)) {
      throw const FormatException('Duplicate processor preset id.');
    }
    out.add(preset);
  }
  return List<ProcessorPreset>.unmodifiable(out);
}

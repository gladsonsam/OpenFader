// Device library for processor (EQ/gate/compressor) presets.
//
// SharedPreferences-backed, per-device store behind the pure contracts in
// `lib/core/processor_presets.dart`. No hardware, no network, no timers.
// The main `AppStore` instantiates one `ProcessorPresetLibrary` and uses it
// to implement `ProcessorPresetStore`; Settings `fullReset` clears it via
// [clear] (owned by the parent, not done here).
//
// Guarantees:
// - Empty until the user saves; no factory presets are invented.
// - Reads/writes throw on persistence or corruption errors; corruption is
//   never silently emptied or overwritten (mutations load first and abort).
// - `save` upserts by id and rejects duplicate names within the same
//   `driver` + `family` + `kind` scope (trimmed, case-insensitive).
// - Concurrent mutations are serialized; every write awaits `setString` and
//   throws on `false`.
library;

import 'dart:async';

import 'package:shared_preferences/shared_preferences.dart';

import '../core/processor_presets.dart';

/// SharedPreferences-backed device library for [ProcessorPreset]s.
class ProcessorPresetLibrary {
  ProcessorPresetLibrary();

  // Serializes concurrent mutations so two saves/renames/deletes cannot
  // interleave a load-modify-write cycle. Failures do not break the chain.
  Future<void> _tail = Future.value();

  Future<T> _serialized<T>(Future<T> Function() work) {
    final next = _tail.then((_) => work());
    _tail = next.then((_) {}, onError: (_) {});
    return next;
  }

  static String _scopeKey(ProcessorPreset preset) =>
      '${preset.driver}\x00${preset.family}\x00${preset.kind.wire}';

  static bool _sameName(String a, String b) =>
      a.trim().toLowerCase() == b.trim().toLowerCase();

  /// Load every stored preset (all scopes). Throws on persistence or
  /// corruption errors; returns `[]` only when nothing was ever stored.
  Future<List<ProcessorPreset>> load() async {
    final prefs = await SharedPreferences.getInstance();
    return decodeProcessorPresets(prefs.getString(kProcessorPresetStorageKey));
  }

  /// Insert or replace [preset] by id. Rejects duplicate display names
  /// within the same `driver` + `family` + `kind` scope with [StateError].
  /// Throws on validation, persistence, or corruption errors.
  Future<void> save(ProcessorPreset preset) {
    return _serialized(() async {
      final nameError = validateProcessorPresetName(preset.name);
      if (nameError != null) throw StateError(nameError);
      // Re-validate through the strict codec so unknown fields and bad
      // primitives can never be persisted, even from hand-built presets.
      final canonical = ProcessorPreset.fromJson(preset.toJson()).copyWith();
      final prefs = await SharedPreferences.getInstance();
      final current = decodeProcessorPresets(
        prefs.getString(kProcessorPresetStorageKey),
      );
      final scope = _scopeKey(canonical);
      for (final other in current) {
        if (other.id != canonical.id &&
            _scopeKey(other) == scope &&
            _sameName(other.name, canonical.name)) {
          throw StateError('A preset with this name already exists.');
        }
      }
      final next = <ProcessorPreset>[
        for (final other in current)
          if (other.id != canonical.id) other,
        canonical,
      ];
      await _persist(prefs, next);
    });
  }

  /// Rename preset [id] to [name] (trimmed, Unicode-preserving). Throws
  /// [StateError] when the id is unknown, the name is invalid, or another
  /// preset in the same `driver` + `family` + `kind` scope already uses it.
  Future<void> rename(String id, String name) {
    return _serialized(() async {
      final nameError = validateProcessorPresetName(name);
      if (nameError != null) throw StateError(nameError);
      final trimmed = name.trim();
      final prefs = await SharedPreferences.getInstance();
      final current = decodeProcessorPresets(
        prefs.getString(kProcessorPresetStorageKey),
      );
      var found = false;
      var scope = '';
      for (final preset in current) {
        if (preset.id == id) {
          found = true;
          scope = _scopeKey(preset);
          break;
        }
      }
      if (!found) throw StateError('Preset not found.');
      for (final other in current) {
        if (other.id != id &&
            _scopeKey(other) == scope &&
            _sameName(other.name, trimmed)) {
          throw StateError('A preset with this name already exists.');
        }
      }
      final next = <ProcessorPreset>[
        for (final preset in current)
          if (preset.id == id)
            preset.copyWith(name: trimmed, updatedAt: DateTime.now())
          else
            preset,
      ];
      await _persist(prefs, next);
    });
  }

  /// Delete preset [id]. No-op when the id is unknown.
  Future<void> delete(String id) {
    return _serialized(() async {
      final prefs = await SharedPreferences.getInstance();
      final current = decodeProcessorPresets(
        prefs.getString(kProcessorPresetStorageKey),
      );
      if (!current.any((preset) => preset.id == id)) return;
      await _persist(
        prefs,
        [
          for (final preset in current)
            if (preset.id != id) preset
        ],
      );
    });
  }

  /// Remove every stored preset (used by Settings full reset). Throws on
  /// persistence errors.
  Future<void> clear() {
    return _serialized(() async {
      final prefs = await SharedPreferences.getInstance();
      if (!await prefs.remove(kProcessorPresetStorageKey)) {
        await prefs.reload();
        throw StateError('Processor presets could not be cleared.');
      }
    });
  }

  Future<void> _persist(
    SharedPreferences prefs,
    List<ProcessorPreset> presets,
  ) async {
    final raw = encodeProcessorPresets(presets);
    if (!await prefs.setString(kProcessorPresetStorageKey, raw)) {
      await prefs.reload();
      throw StateError('Processor presets could not be saved.');
    }
  }
}

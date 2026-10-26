// Typed app preferences over SharedPreferences. One instance lives for the
// app session (`PrefsService.instance`); UI reads it directly and listens
// for the knobs that need live apply.
library;

import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:wakelock_plus/wakelock_plus.dart';

class PrefsService extends ChangeNotifier {
  PrefsService._();
  static final PrefsService instance = PrefsService._();
  static Future<void>? _initializing;

  SharedPreferences? _prefs;
  bool get loaded => _prefs != null;

  static Future<void> ensureInitialized() => _initializing ??= _initialize();
  static Future<void> _initialize() async {
    try {
      instance._prefs = await SharedPreferences.getInstance();
    } catch (_) {
      _initializing = null;
      rethrow;
    }
    instance._applyWakeLock();
    instance.notifyListeners();
  }

  /// One installation-wide policy, deliberately independent of mixer/session.
  Set<String> get blockedAccess {
    final raw = _prefs?.getString('openfader.accessLimits.v1');
    if (raw == null) return const {};
    try {
      final values = jsonDecode(raw);
      if (values is List && values.every((v) => v is String)) {
        return Set.unmodifiable(values.cast<String>());
      }
    } catch (_) {
      // Corrupt local data must not crash startup. No remote security claim.
    }
    return const {};
  }

  Future<void> setBlockedAccess(Set<String> keys) =>
      _set('openfader.accessLimits.v1', jsonEncode(keys.toList()..sort()));

  // --- connection ----------------------------------------------------------
  bool get autoFind => _prefs?.getBool('openfader.autoFind') ?? true;
  bool get reconnectLast => _prefs?.getBool('openfader.reconnectLast') ?? false;
  bool get rememberRecents =>
      _prefs?.getBool('openfader.rememberRecents') ?? true;
  String get startupDest =>
      _prefs?.getString('openfader.startupDest') ?? 'connections';
  String get lastMixerAddr =>
      _prefs?.getString('openfader.lastMixerAddr') ?? '';

  // --- appearance -----------------------------------------------------------
  String get appearance =>
      _choice('openfader.appearance', ['dark', 'light', 'system'], 'dark');
  String get accent =>
      _choice('openfader.accent', ['lime', 'ocean', 'violet', 'amber'], 'lime');
  String _choice(String key, List<String> choices, String fallback) {
    final value = _prefs?.getString(key);
    return choices.contains(value) ? value! : fallback;
  }

  String get interfaceSize =>
      _prefs?.getString('openfader.interfaceSize') ?? 'comfortable';
  bool get keepAwake => _prefs?.getBool('openfader.keepAwake') ?? true;
  double get textScale => switch (interfaceSize) {
        'compact' => 0.9,
        'large' => 1.12,
        _ => 1.0,
      };

  // --- meters ----------------------------------------------------------------
  int get meterRate => _prefs?.getInt('openfader.meterRate') ?? 30;
  int get peakHold => _prefs?.getInt('openfader.peakHold') ?? 1;
  bool get holdClips => _prefs?.getBool('openfader.holdClips') ?? true;
  bool get largeTargets => _prefs?.getBool('openfader.largeTargets') ?? true;

  // --- mixes ------------------------------------------------------------------
  bool get autoBackup => _prefs?.getBool('openfader.autoBackup') ?? true;
  String get backupCreatedAt =>
      _prefs?.getString('openfader.backupCreatedAt') ?? '';

  Future<void> _set<T>(String key, T value) async {
    final p = _prefs ?? await SharedPreferences.getInstance();
    _prefs ??= p;
    final bool saved;
    try {
      if (value is bool) {
        saved = await p.setBool(key, value);
      } else if (value is int) {
        saved = await p.setInt(key, value);
      } else {
        saved = await p.setString(key, value as String);
      }
    } catch (_) {
      // SharedPreferences updates its cache before calling the platform. A
      // thrown save must recover that cache just like a false save result.
      await p.reload();
      notifyListeners();
      rethrow;
    }
    if (!saved) {
      await p.reload();
      notifyListeners();
      throw StateError("The preference could not be saved.");
    }
    notifyListeners();
  }

  Future<void> setAutoFind(bool v) => _set('openfader.autoFind', v);
  Future<void> setReconnectLast(bool v) => _set('openfader.reconnectLast', v);
  Future<void> setRemember(bool v) async {
    await _set('openfader.rememberRecents', v);
  }

  Future<void> setStartupDest(String v) => _set('openfader.startupDest', v);
  Future<void> setLastMixerAddr(String v) => _set('openfader.lastMixerAddr', v);
  Future<void> setAppearance(String v) {
    if (!['dark', 'light', 'system'].contains(v)) {
      throw ArgumentError.value(v);
    }
    return _set('openfader.appearance', v);
  }

  Future<void> setAccent(String v) {
    if (!['lime', 'ocean', 'violet', 'amber'].contains(v)) {
      throw ArgumentError.value(v);
    }
    return _set('openfader.accent', v);
  }

  Future<void> setInterfaceSize(String v) => _set('openfader.interfaceSize', v);
  Future<void> setKeepAwake(bool v) async {
    await _set('openfader.keepAwake', v);
    _applyWakeLock();
  }

  Future<void> setMeterRate(int v) => _set('openfader.meterRate', v);
  Future<void> setPeakHold(int v) => _set('openfader.peakHold', v);
  Future<void> setHoldClips(bool v) => _set('openfader.holdClips', v);
  Future<void> setLargeTargets(bool v) => _set('openfader.largeTargets', v);
  Future<void> setAutoBackup(bool v) => _set('openfader.autoBackup', v);
  Future<void> setBackupCreatedAt(String v) =>
      _set('openfader.backupCreatedAt', v);

  void _applyWakeLock() {
    WakelockPlus.toggle(enable: keepAwake);
  }

  /// Restore display, meter, and connection defaults. Access limits, mixes,
  /// recents, and addresses are untouched (separate destructive actions own those).
  Future<void> resetToDefaults() async {
    final p = _prefs ?? await SharedPreferences.getInstance();
    _prefs ??= p;
    for (final key in [
      'openfader.autoFind',
      'openfader.reconnectLast',
      'openfader.rememberRecents',
      'openfader.startupDest',
      'openfader.appearance',
      'openfader.accent',
      'openfader.interfaceSize',
      'openfader.keepAwake',
      'openfader.meterRate',
      'openfader.peakHold',
      'openfader.holdClips',
      'openfader.largeTargets',
      'openfader.autoBackup',
    ]) {
      if (!await p.remove(key)) {
        await p.reload();
        notifyListeners();
        throw StateError("The preference could not be removed.");
      }
    }
    _applyWakeLock();
    notifyListeners();
  }

  /// Full wipe of every openfader.* key (Reset all app data).
  Future<void> clearAll() async {
    final p = _prefs ?? await SharedPreferences.getInstance();
    _prefs ??= p;
    for (final key in p.getKeys().where((k) => k.startsWith('openfader.'))) {
      if (!await p.remove(key)) {
        await p.reload();
        notifyListeners();
        throw StateError("The preference could not be removed.");
      }
    }
    _applyWakeLock();
    notifyListeners();
  }
}

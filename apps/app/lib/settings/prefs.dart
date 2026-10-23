// Typed app preferences over SharedPreferences. One instance lives for
// the app session (`PrefsService.instance`); UI reads it directly and the
// shell/pages listen for the knobs that need live apply (meter cadence,
// wake lock, interface density).
library;

import 'package:flutter/foundation.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:wakelock_plus/wakelock_plus.dart';

class PrefsService extends ChangeNotifier {
  PrefsService._();
  static final PrefsService instance = PrefsService._();
  static bool _initStarted = false;

  SharedPreferences? _prefs;
  bool get loaded => _prefs != null;

  static void ensureInitialized() {
    if (_initStarted) return;
    _initStarted = true;
    SharedPreferences.getInstance().then((p) {
      instance._prefs = p;
      instance._applyWakeLock();
      instance.notifyListeners();
    });
  }

  // --- connection ----------------------------------------------------------
  bool get autoFind => _prefs?.getBool('openfader.autoFind') ?? true;
  bool get reconnectLast =>
      _prefs?.getBool('openfader.reconnectLast') ?? false;
  bool get rememberRecents =>
      _prefs?.getBool('openfader.rememberRecents') ?? true;
  String get startupDest =>
      _prefs?.getString('openfader.startupDest') ?? 'connections';
  String get lastMixerAddr =>
      _prefs?.getString('openfader.lastMixerAddr') ?? '';

  // --- appearance -----------------------------------------------------------
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
  bool get largeTargets =>
      _prefs?.getBool('openfader.largeTargets') ?? true;

  // --- mixes ------------------------------------------------------------------
  bool get autoBackup => _prefs?.getBool('openfader.autoBackup') ?? true;
  String get backupCreatedAt =>
      _prefs?.getString('openfader.backupCreatedAt') ?? '';

  Future<void> _set<T>(String key, T value) async {
    final p = _prefs ?? await SharedPreferences.getInstance();
    _prefs ??= p;
    if (value is bool) {
      await p.setBool(key, value);
    } else if (value is int) {
      await p.setInt(key, value);
    } else {
      await p.setString(key, value as String);
    }
    notifyListeners();
  }

  Future<void> setAutoFind(bool v) =>
      _set('openfader.autoFind', v);
  Future<void> setReconnectLast(bool v) =>
      _set('openfader.reconnectLast', v);
  Future<void> setRemember(bool v) async {
    await _set('openfader.rememberRecents', v);
  }

  Future<void> setStartupDest(String v) =>
      _set('openfader.startupDest', v);
  Future<void> setLastMixerAddr(String v) =>
      _set('openfader.lastMixerAddr', v);
  Future<void> setInterfaceSize(String v) =>
      _set('openfader.interfaceSize', v);
  Future<void> setKeepAwake(bool v) async {
    await _set('openfader.keepAwake', v);
    _applyWakeLock();
  }

  Future<void> setMeterRate(int v) => _set('openfader.meterRate', v);
  Future<void> setPeakHold(int v) => _set('openfader.peakHold', v);
  Future<void> setHoldClips(bool v) => _set('openfader.holdClips', v);
  Future<void> setLargeTargets(bool v) =>
      _set('openfader.largeTargets', v);
  Future<void> setAutoBackup(bool v) =>
      _set('openfader.autoBackup', v);
  Future<void> setBackupCreatedAt(String v) =>
      _set('openfader.backupCreatedAt', v);

  void _applyWakeLock() {
    WakelockPlus.toggle(enable: keepAwake);
  }

  /// Restore all display/meter/connection defaults. Mixes, recents and
  /// addresses are untouched (separate destructive actions own those).
  Future<void> resetToDefaults() async {
    final p = _prefs ?? await SharedPreferences.getInstance();
    _prefs ??= p;
    for (final key in [
      'openfader.autoFind',
      'openfader.reconnectLast',
      'openfader.rememberRecents',
      'openfader.startupDest',
      'openfader.interfaceSize',
      'openfader.keepAwake',
      'openfader.meterRate',
      'openfader.peakHold',
      'openfader.holdClips',
      'openfader.largeTargets',
      'openfader.autoBackup',
    ]) {
      await p.remove(key);
    }
    _applyWakeLock();
    notifyListeners();
  }

  /// Full wipe of every openfader.* key (used by Reset all app data).
  Future<void> clearAll() async {
    final p = _prefs ?? await SharedPreferences.getInstance();
    _prefs ??= p;
    for (final key in p.getKeys().where((k) => k.startsWith('openfader.'))) {
      await p.remove(key);
    }
    _applyWakeLock();
    notifyListeners();
  }
}

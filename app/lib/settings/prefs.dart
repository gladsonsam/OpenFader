// Typed app preferences over SharedPreferences. One instance lives for the
// app session (`PrefsService.instance`); UI reads it directly and listens
// for the knobs that need live apply.
library;

import 'dart:async';
import 'dart:convert';

import 'package:flutter/foundation.dart';
import '../mixer/store.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:wakelock_plus/wakelock_plus.dart';

import '../core/access_policy.dart';
import '../mixer/mixer_layers.dart';
import '../mixer/strip_layout.dart';
import 'channel_icons.dart';

class PrefsService extends ChangeNotifier {
  PrefsService._();
  static final PrefsService instance = PrefsService._();
  static Future<void>? _initializing;

  SharedPreferences? _prefs;
  ChannelIconRepository? channelIcons;
  bool get loaded => _prefs != null;

  static Future<void> ensureInitialized() => _initializing ??= _initialize();
  static Future<void> _initialize() async {
    try {
      instance._prefs = await SharedPreferences.getInstance();
      instance.channelIcons = ChannelIconRepository(instance._prefs!);
      await instance.channelIcons!.initialize();
    } catch (_) {
      _initializing = null;
      rethrow;
    }
    instance._accessPolicy = null;
    instance._accessRevision++;
    instance._applyWakeLock();
    instance.notifyListeners();
  }

  static const _accessKey = 'openfader.accessPolicy.v2';
  AccessPolicy? _accessPolicy;
  bool _accessSaving = false;
  int _accessRevision = 0;
  bool get accessSaving => _accessSaving;
  int get accessRevision => _accessRevision;

  /// Read a committed snapshot, never SharedPreferences' optimistic cache.
  AccessPolicy get accessPolicy => _accessPolicy ??= AccessPolicy.decode(
        _prefs?.getString(_accessKey),
        _prefs?.getString('openfader.accessLimits.v1'),
      );
  Set<String> get blockedAccess => accessPolicy.blocked;

  void requireAccessEditable() {
    if (accessSaving || accessPolicy.locked) {
      throw StateError('Unlock this device’s access limits first.');
    }
  }

  Future<void> _saveAccess(AccessPolicy next) async {
    if (_accessSaving) throw StateError('An access change is already saving.');
    _accessSaving = true;
    // Invalidate captures and queued work even when persistence later fails.
    _accessRevision++;
    notifyListeners();
    try {
      await _set(_accessKey, next.encode());
      _accessPolicy = next;
    } finally {
      _accessSaving = false;
      _accessRevision++;
      notifyListeners();
    }
  }

  Future<void> setBlockedAccess(Set<String> keys) async {
    await ensureInitialized();
    requireAccessEditable();
    await _saveAccess(accessPolicy.copyWith(blocked: keys));
  }

  Future<void> setAccessPasscode(String passcode, String confirmation) async {
    await ensureInitialized();
    requireAccessEditable();
    if (accessPolicy.hasPasscode) {
      throw StateError('Use Change passcode for an existing passcode.');
    }
    AccessPolicy.validatePasscode(passcode, confirmation);
    await _saveAccess(accessPolicy.copyWith(passcode: passcode, locked: true));
  }

  Future<void> lockAccess() async {
    await ensureInitialized();
    requireAccessEditable();
    if (!accessPolicy.hasPasscode) throw StateError('Set a passcode first.');
    await _saveAccess(accessPolicy.copyWith(locked: true));
  }

  Future<void> unlockAccess(String passcode) async {
    await ensureInitialized();
    accessPolicy.requirePasscode(passcode);
    await _saveAccess(accessPolicy.copyWith(locked: false));
  }

  Future<void> changeAccessPasscode(
    String current,
    String next,
    String confirmation,
  ) async {
    await ensureInitialized();
    requireAccessEditable();
    accessPolicy.requirePasscode(current);
    AccessPolicy.validatePasscode(next, confirmation);
    await _saveAccess(accessPolicy.copyWith(passcode: next));
  }

  Future<void> removeAccessPasscode(String current) async {
    await ensureInitialized();
    requireAccessEditable();
    accessPolicy.requirePasscode(current);
    await _saveAccess(accessPolicy.copyWith(passcode: ''));
  }

  // --- connection ----------------------------------------------------------
  bool get autoFind => _prefs?.getBool('openfader.autoFind') ?? true;
  bool get reconnectLast => _prefs?.getBool('openfader.reconnectLast') ?? true;

  /// False after an explicit disconnect until the next successful hardware
  /// connect. Keeps a cold start from re-dialling a desk the user just left.
  bool get reconnectArmed =>
      _prefs?.getBool('openfader.reconnectArmed') ?? true;
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

  // --- channel strips -------------------------------------------------------
  static const stripLayoutKey = 'openfader.stripLayout.v1';

  // Trailing-edge debounce for per-tick drag writes (e.g. the fader-size
  // slider): the in-memory layout updates immediately so the UI stays live,
  // while SharedPreferences persistence waits ~400ms after the last tick.
  StripLayout? _pendingStripLayout;
  Timer? _stripLayoutTimer;
  static const _stripLayoutDebounce = Duration(milliseconds: 400);

  /// Parsed strip layout with defaults fallback on missing/corrupt payloads.
  /// A pending debounced write (if any) wins over the persisted payload.
  StripLayout get stripLayout =>
      _pendingStripLayout ??
      StripLayout.tryDecode(_prefs?.getString(stripLayoutKey)) ??
      StripLayout.defaults();

  Future<void> setStripLayout(StripLayout layout) {
    _stripLayoutTimer?.cancel();
    _stripLayoutTimer = null;
    _pendingStripLayout = null;
    return _set(stripLayoutKey, layout.encode());
  }

  /// Debounced variant for drag/slider ticks: applies [layout] in memory
  /// now (notifyListeners) and persists the latest value on the trailing
  /// edge. Follows the same [_set] persistence path, so save failures still
  /// reload + notify. The singleton lives for the app session, so there is
  /// no dispose to cancel the timer; an immediate [setStripLayout] cancels
  /// any pending trailing write.
  void setStripLayoutDebounced(StripLayout layout) {
    _pendingStripLayout = layout;
    notifyListeners();
    _stripLayoutTimer?.cancel();
    _stripLayoutTimer = Timer(_stripLayoutDebounce, () async {
      final pending = _pendingStripLayout;
      _pendingStripLayout = null;
      _stripLayoutTimer = null;
      if (pending != null) {
        await setStripLayout(pending);
      }
    });
  }

  // --- mixer layers ---------------------------------------------------------
  static const layersKey = 'openfader.layers.v1';
  static const activeLayerIdKey = 'openfader.activeLayerId';

  /// User layers with defaults fallback on missing/corrupt payloads.
  List<MixerLayer> get mixerLayers =>
      MixerLayer.tryDecodeList(_prefs?.getString(layersKey)) ?? const [];

  /// Active layer id; unknown ids fall back to the built-in All layer.
  String get activeMixerLayerId =>
      _prefs?.getString(activeLayerIdKey) ?? MixerLayer.allLayerId;

  MixerLayersState get mixerLayersState => MixerLayersState(
        layers: mixerLayers,
        activeLayerId: activeMixerLayerId,
      );

  Future<void> setMixerLayers(List<MixerLayer> layers) =>
      _set(layersKey, MixerLayer.encodeList(layers));

  Future<void> setActiveMixerLayerId(String id) => _set(activeLayerIdKey, id);

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

  List<CustomMixerView> mixerViews(String scope) {
    try {
      final data =
          jsonDecode(_prefs?.getString('openfader.mixerViews.$scope') ?? '[]');
      if (data is! List) return const [];
      return [
        for (final row in data)
          if (row is Map &&
              row['id'] is String &&
              row['name'] is String &&
              row['strips'] is List)
            CustomMixerView(
                id: row['id'] as String,
                name: row['name'] as String,
                stripKeys: (row['strips'] as List).whereType<String>().toSet())
      ];
    } catch (_) {
      return const [];
    }
  }

  Future<void> saveMixerViews(String scope, List<CustomMixerView> views) =>
      _set(
          'openfader.mixerViews.$scope',
          jsonEncode([
            for (final view in views)
              {
                'id': view.id,
                'name': view.name,
                'strips': view.stripKeys.toList()
              }
          ]));

  Future<void> setAutoFind(bool v) => _set('openfader.autoFind', v);
  Future<void> setReconnectLast(bool v) => _set('openfader.reconnectLast', v);
  Future<void> setReconnectArmed(bool v) => _set('openfader.reconnectArmed', v);
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
    unawaited(WakelockPlus.toggle(enable: keepAwake).catchError((Object error) {
      // Some desktop sessions have no inhibition portal. That must not stop
      // startup or leave a preference change as an unhandled async error.
      debugPrint('Keep-awake is unavailable in this desktop session: $error');
    }));
  }

  /// Restore display, meter, and connection defaults. Access limits, mixes,
  /// recents, and addresses are untouched (separate destructive actions own those).
  Future<void> resetToDefaults() async {
    final p = _prefs ?? await SharedPreferences.getInstance();
    _prefs ??= p;
    for (final key in [
      'openfader.autoFind',
      'openfader.reconnectLast',
      'openfader.reconnectArmed',
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
      stripLayoutKey,
      layersKey,
      activeLayerIdKey,
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

  /// Reset app data while preserving access policy. Only authenticated access
  /// controls can remove its passcode; forgotten codes require an OS app reset.
  Future<void> clearAll() async {
    await ensureInitialized();
    requireAccessEditable();
    final icons = channelIcons;
    await icons?.prepareForReset();
    final p = _prefs ?? await SharedPreferences.getInstance();
    _prefs ??= p;
    for (final key in p.getKeys().where(
          (k) =>
              k.startsWith('openfader.') &&
              k != _accessKey &&
              k != 'openfader.accessLimits.v1',
        )) {
      if (!await p.remove(key)) {
        await p.reload();
        notifyListeners();
        throw StateError("The preference could not be removed.");
      }
    }
    await icons?.removeManagedImages();
    channelIcons = ChannelIconRepository(p);
    await channelIcons!.initialize();
    _applyWakeLock();
    notifyListeners();
  }
}

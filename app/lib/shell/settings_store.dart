import 'package:flutter/widgets.dart';
// Shell adapter: settings responsibility. The shell owns polling.
import 'dart:convert';
import 'dart:io';
import 'package:share_plus/share_plus.dart';

import '../core/access_limits.dart';
import '../settings/mixes.dart';
import '../settings/prefs.dart';
import '../settings/recents.dart';
import '../screens/settings.dart';
import 'store_adapter.dart';

class ShellSettingsStore extends ShellStoreAdapter implements SettingsStore {
  ShellSettingsStore(super.app) {
    prefs.addListener(forwardChange);
    recents.addListener(forwardChange);
    mixes.addListener(forwardChange);
    recents.load();
    mixes.refresh();
  }

  @override
  void dispose() {
    prefs.removeListener(forwardChange);
    recents.removeListener(forwardChange);
    mixes.removeListener(forwardChange);
    super.dispose();
  }

  @override
  AccessLimitsStore get accessLimits => app;

  @override
  PrefsService get prefs => PrefsService.instance;

  final RecentMixers _recents = RecentMixers.instance;

  @override
  RecentMixers get recents {
    RecentMixers.ensureLoaded();
    return _recents;
  }

  final LocalMixStore _mixes = LocalMixStore();

  @override
  LocalMixStore get mixes => _mixes;

  @override
  Future<int> appStorageBytes() async =>
      await _mixes.storageBytes() +
      await app.processorPresetStorageBytes() +
      await app.localLibraryStorageBytes();

  @override
  Future<String?> importMix() async {
    final err = await _mixes.importMix();
    if (err == null) {
      await _snapshotBackup();
      notifyListeners();
    }
    return err;
  }

  @override
  Future<void> exportMix(LocalMix mix) => _mixes.exportMix(mix);

  @override
  Future<void> deleteMix(LocalMix mix) async {
    await _mixes.deleteMix(mix);
    await _snapshotBackup();
    notifyListeners();
  }

  @override
  Future<String> importBackup() async {
    final res = await _mixes.importBackup();
    if (res != 'cancelled') {
      await _snapshotBackup();
      notifyListeners();
    }
    return res;
  }

  @override
  Future<void> exportBackup() => _mixes.exportBackup();

  Future<void> _snapshotBackup() async {
    if (!prefs.autoBackup) return;
    try {
      await _mixes.snapshotBackup();
      await prefs.setBackupCreatedAt(DateTime.now().toIso8601String());
    } catch (error) {
      throw StateError(
          "The mix change was saved, but its automatic backup failed: $error");
    }
  }

  @override
  Future<void> clearCache() async {
    await _mixes.clearCache();
    notifyListeners();
  }

  @override
  Future<void> forgetMixers() async {
    await app.forgetSavedMixers();
    await _recents.clear();
    notifyListeners();
  }

  @override
  Future<void> resetPrefs() async {
    await prefs.resetToDefaults();
    notifyListeners();
  }

  @override
  Future<void> deleteMixes() async {
    await _mixes.deleteAll();
    notifyListeners();
  }

  @override
  Future<void> fullReset() async {
    await PrefsService.ensureInitialized();
    prefs.requireAccessEditable();
    await app.resetLocalLibraryThen(() async {
      await app.forgetSavedMixers();
      await app.clearProcessorPresets();
      await _mixes.deleteAll();
      await _mixes.clearCache();
      await _recents.clear();
      await prefs.clearAll();
      notifyListeners();
    });
  }

  @override
  Future<void> exportDiagnostics() async {
    final payload = {
      'app': 'OpenFader',
      'generatedAt': DateTime.now().toIso8601String(),
      'preferences': {
        'autoFind': prefs.autoFind,
        'reconnectLast': prefs.reconnectLast,
        'rememberRecents': prefs.rememberRecents,
        'startupDest': prefs.startupDest,
        'interfaceSize': prefs.interfaceSize,
        'keepAwake': prefs.keepAwake,
        'meterRate': prefs.meterRate,
        'peakHold': prefs.peakHold,
        'holdClips': prefs.holdClips,
        'largeTargets': prefs.largeTargets,
        'autoBackup': prefs.autoBackup,
      },
      'localMixCount': _mixes.mixes.length,
      'savedMixerCount': _recents.items.length,
    };
    final tmp = Directory.systemTemp;
    final file = File('${tmp.path}/open-fader-diagnostics.json');
    await file.writeAsString(jsonEncode(payload));
    final view = WidgetsBinding.instance.platformDispatcher.views.first;
    final size = view.physicalSize / view.devicePixelRatio;
    final result = await SharePlus.instance.share(ShareParams(
        files: [XFile(file.path)],
        subject: 'open-fader-diagnostics.json',
        sharePositionOrigin:
            Rect.fromLTWH(size.width / 2, size.height / 2, 1, 1)));
    if (result.status == ShareResultStatus.dismissed) {
      throw MixExportCancelled();
    }
    if (result.status != ShareResultStatus.success) {
      throw StateError(
          'The platform could not confirm the diagnostics export.');
    }
  }
}

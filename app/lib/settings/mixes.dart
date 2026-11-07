// Local mixes and backups on this device. Mixes arrive only through import
// (there is no verified over-the-wire mix format) and each file is validated
// before it is stored. Nothing here touches mixer hardware.
library;

import 'dart:convert';
import 'dart:io';
import 'dart:ui' show PlatformDispatcher, Rect;

import 'package:file_picker/file_picker.dart';
import 'package:flutter/foundation.dart';
import 'package:path_provider/path_provider.dart';
import 'package:share_plus/share_plus.dart';

/// Dismissing the platform share sheet is not a successful export.
class MixExportCancelled implements Exception {}

class LocalMix {
  final String name;
  final String file;
  final DateTime createdAt;
  final String mixerModel;

  const LocalMix(
      {required this.name,
      required this.file,
      required this.createdAt,
      required this.mixerModel});
}

class MixValidation {
  final Map<String, dynamic>? mix;
  final String? error;
  const MixValidation.ok(this.mix) : error = null;
  const MixValidation.bad(this.error) : mix = null;
}

class LocalMixStore extends ChangeNotifier {
  String? get unavailableReason => kIsWeb
      ? 'Local mix files and backups are available in the desktop and mobile apps. Browser controls use the connected server.'
      : null;

  void _requireLocalFiles() {
    if (kIsWeb) throw UnsupportedError(unavailableReason!);
  }

  List<LocalMix> _mixes = [];
  List<LocalMix> get mixes => List.unmodifiable(_mixes);

  Future<Directory> _dir() async {
    _requireLocalFiles();
    final docs = await getApplicationDocumentsDirectory();
    final dir = Directory('${docs.path}/openfader_mixes');
    if (!await dir.exists()) await dir.create(recursive: true);
    return dir;
  }

  Future<File> _backupFile() async {
    _requireLocalFiles();
    final docs = await getApplicationDocumentsDirectory();
    return File('${docs.path}/openfader_backup.json');
  }

  static String sanitize(String name) {
    final clean =
        name.trim().replaceAll(RegExp(r'[^A-Za-z0-9 _-]+'), '').trim();
    if (clean.isEmpty) return 'mix';
    return clean.length > 80 ? clean.substring(0, 80) : clean;
  }

  /// Validation shared by mix and backup imports.
  static MixValidation validateMix(dynamic decoded) {
    if (decoded is! Map<String, dynamic>) {
      return const MixValidation.bad('Not valid JSON.');
    }
    final rawName = decoded['name'];
    if (rawName is! String) {
      return const MixValidation.bad('Choose a mix JSON file with a name.');
    }
    final name = rawName.trim();
    if (name.isEmpty || name.length > 160) {
      return const MixValidation.bad('Choose a mix JSON file with a name.');
    }
    return MixValidation.ok(decoded);
  }

  Future<void> refresh() async {
    if (kIsWeb) return;
    final dir = await _dir();
    final out = <LocalMix>[];
    await for (final e in dir.list()) {
      if (e is! File || !e.path.endsWith('.json')) continue;
      try {
        final decoded = jsonDecode(await e.readAsString());
        if (decoded is! Map<String, dynamic>) continue;
        final name = '${decoded['name'] ?? ''}';
        if (name.isEmpty) continue;
        final created = DateTime.tryParse('${decoded['createdAt'] ?? ''}') ??
            DateTime.fromMillisecondsSinceEpoch(0);
        out.add(LocalMix(
            name: name,
            file: e.uri.pathSegments.last,
            createdAt: created,
            mixerModel: '${decoded['mixerModel'] ?? 'Local mix'}'));
      } catch (_) {
        // Skip unreadable files; never crash the list.
      }
    }
    out.sort((a, b) => b.createdAt.compareTo(a.createdAt));
    _mixes = out;
    notifyListeners();
  }

  Future<String?> importMix() async {
    _requireLocalFiles();
    final picked = await FilePicker.pickFiles(
        type: FileType.custom, allowedExtensions: ['json']);
    final file = picked.firstOrNull;
    if (file == null) return 'cancelled';
    final bytes = await file.readAsBytes();
    if (bytes.lengthInBytes > 5 * 1024 * 1024) {
      return 'File is larger than 5 MB.';
    }
    dynamic decoded;
    try {
      decoded = jsonDecode(utf8.decode(bytes));
    } catch (_) {
      return 'Not valid JSON.';
    }
    final check = validateMix(decoded);
    if (check.mix == null) return check.error;
    final dir = await _dir();
    final name = sanitize('${check.mix!['name']}');
    final stamped = {
      ...check.mix!,
      'name': '${check.mix!['name']}'.trim(),
      'createdAt': DateTime.now().toIso8601String(),
    };
    var target = File('${dir.path}/$name.json');
    var n = 1;
    while (await target.exists()) {
      n++;
      target = File('${dir.path}/$name $n.json');
    }
    await target.writeAsString(jsonEncode(stamped));
    await refresh();
    return null;
  }

  Future<void> exportMix(LocalMix mix) async {
    final dir = await _dir();
    final file = File('${dir.path}/${mix.file}');
    if (!await file.exists()) {
      throw FileSystemException('The mix file is missing.', file.path);
    }
    await _shareFile(file, '${mix.name}.json');
  }

  Future<void> _shareFile(File file, String subject) async {
    // iPad requires a nonempty popover anchor in the presenting window.
    final view = PlatformDispatcher.instance.views.first;
    final size = view.physicalSize / view.devicePixelRatio;
    final result = await SharePlus.instance.share(ShareParams(
        files: [XFile(file.path)],
        subject: subject,
        sharePositionOrigin:
            Rect.fromLTWH(size.width / 2, size.height / 2, 1, 1)));
    if (result.status == ShareResultStatus.dismissed) {
      throw MixExportCancelled();
    }
    if (result.status != ShareResultStatus.success) {
      throw StateError('The platform could not confirm the export.');
    }
  }

  Future<void> deleteMix(LocalMix mix) async {
    final dir = await _dir();
    final file = File('${dir.path}/${mix.file}');
    if (await file.exists()) await file.delete();
    await refresh();
  }

  Future<void> deleteAll() async {
    final dir = await _dir();
    await for (final e in dir.list()) {
      if (e is File && e.path.endsWith('.json')) {
        await e.delete();
      }
    }
    final backup = await _backupFile();
    if (await backup.exists()) {
      await backup.delete();
    }
    await refresh();
  }

  Future<Map<String, dynamic>> _backupPayload() async {
    final dir = await _dir();
    final mixes = <Map<String, dynamic>>[];
    await for (final e in dir.list()) {
      if (e is! File || !e.path.endsWith('.json')) continue;
      final decoded = jsonDecode(await e.readAsString());
      final check = validateMix(decoded);
      if (check.mix == null) {
        throw FormatException(
            'Cannot back up ${e.uri.pathSegments.last}: ${check.error}');
      }
      mixes.add(check.mix!);
      if (mixes.length > 500) {
        throw const FormatException('A backup can contain at most 500 mixes.');
      }
    }
    return {
      'format': 'open-fader-backup',
      'version': 1,
      'createdAt': DateTime.now().toIso8601String(),
      'mixes': mixes,
    };
  }

  Future<void> snapshotBackup() async {
    final file = await _backupFile();
    await file.writeAsString(jsonEncode(await _backupPayload()));
  }

  Future<String?> exportBackup() async {
    final payload = await _backupPayload();
    final tmp = Directory.systemTemp;
    final file = File('${tmp.path}/open-fader-backup.json');
    await file.writeAsString(jsonEncode(payload));
    await _shareFile(file, 'open-fader-backup.json');
    return null;
  }

  /// Returns the imported count as a string, or an error string.
  Future<String> importBackup() async {
    _requireLocalFiles();
    final picked = await FilePicker.pickFiles(
        type: FileType.custom, allowedExtensions: ['json']);
    final file = picked.firstOrNull;
    if (file == null) return 'cancelled';
    final bytes = await file.readAsBytes();
    if (bytes.lengthInBytes > 5 * 1024 * 1024) {
      return 'File is larger than 5 MB.';
    }
    dynamic decoded;
    try {
      decoded = jsonDecode(utf8.decode(bytes));
    } catch (_) {
      return 'Not valid JSON.';
    }
    if (decoded is! Map<String, dynamic> ||
        decoded['mixes'] is! List ||
        (decoded['mixes'] as List).length > 500) {
      return 'Choose a backup JSON file with mixes.';
    }
    final incoming = decoded['mixes'] as List;
    if (incoming.any((m) => validateMix(m).mix == null)) {
      return 'Choose a backup with a valid mixes list.';
    }
    var count = 0;
    final dir = await _dir();
    for (final m in incoming) {
      final name = sanitize('${(m as Map)['name']}');
      var target = File('${dir.path}/$name.json');
      var n = 1;
      while (await target.exists()) {
        n++;
        target = File('${dir.path}/$name $n.json');
      }
      await target.writeAsString(jsonEncode({
        ...m as Map<String, dynamic>,
        'createdAt': DateTime.now().toIso8601String(),
      }));
      count++;
    }
    await refresh();
    return '$count';
  }

  /// Bytes used by mixes plus backup, counted from real files.
  Future<int> storageBytes() async {
    var total = 0;
    final dir = await _dir();
    await for (final e in dir.list()) {
      if (e is File) total += await e.length();
    }
    final backup = await _backupFile();
    if (await backup.exists()) total += await backup.length();
    return total;
  }

  /// Clears temporary and cache files only; mixes, recents, and prefs stay.
  Future<int> clearCache() async {
    _requireLocalFiles();
    var removed = 0;
    final tmp = await getTemporaryDirectory();
    await for (final entry in tmp.list()) {
      await entry.delete(recursive: true);
      removed++;
    }
    return removed;
  }
}

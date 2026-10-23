// Local mixes + backups on this device. Mixes arrive only through import
// (there is no verified native over-the-wire mix format); each file is
// validated before it is stored. Export/sharing uses the platform share
// sheet and file picker — nothing here touches mixer hardware.
library;

import 'dart:convert';
import 'dart:io';

import 'package:file_picker/file_picker.dart';
import 'package:flutter/foundation.dart';
import 'package:path_provider/path_provider.dart';
import 'package:share_plus/share_plus.dart';

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
  List<LocalMix> _mixes = [];
  List<LocalMix> get mixes => List.unmodifiable(_mixes);

  Future<Directory> _dir() async {
    final docs = await getApplicationDocumentsDirectory();
    final dir = Directory('${docs.path}/openfader_mixes');
    if (!await dir.exists()) await dir.create(recursive: true);
    return dir;
  }

  Future<File> _backupFile() async {
    final docs = await getApplicationDocumentsDirectory();
    return File('${docs.path}/openfader_backup.json');
  }

  static String sanitize(String name) {
    final clean =
        name.trim().replaceAll(RegExp(r'[^A-Za-z0-9 _-]+'), '').trim();
    if (clean.isEmpty) return 'mix';
    return clean.length > 80 ? clean.substring(0, 80) : clean;
  }

  /// JSON-object validation shared by mix + backup imports.
  static MixValidation validateMix(dynamic decoded) {
    if (decoded is! Map<String, dynamic>) {
      return const MixValidation.bad('Not valid JSON.');
    }
    final name = '${decoded['name'] ?? ''}'.trim();
    if (name.isEmpty || name.length > 160) {
      return const MixValidation.bad(
          'Choose a mix JSON file with a name.');
    }
    return MixValidation.ok(decoded);
  }

  Future<void> refresh() async {
    final dir = await _dir();
    final out = <LocalMix>[];
    await for (final e in dir.list()) {
      if (e is! File || !e.path.endsWith('.json')) continue;
      try {
        final decoded = jsonDecode(await e.readAsString());
        if (decoded is! Map<String, dynamic>) continue;
        final name = '${decoded['name'] ?? ''}';
        if (name.isEmpty) continue;
        final created = DateTime.tryParse(
                '${decoded['createdAt'] ?? ''}') ??
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
    final picked = await FilePicker.pickFiles(
        type: FileType.custom, allowedExtensions: ['json']);
    final file = picked.firstOrNull;
    if (file == null) return null; // cancelled
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
    if (!await file.exists()) return;
    await SharePlus.instance.share(ShareParams(files: [XFile(file.path)], subject: '${mix.name}.json'));
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
        try {
          await e.delete();
        } catch (_) {}
      }
    }
    final backup = await _backupFile();
    if (await backup.exists()) {
      try {
        await backup.delete();
      } catch (_) {}
    }
    await refresh();
  }

  Future<Map<String, dynamic>> _backupPayload() async {
    final dir = await _dir();
    final mixes = <Map<String, dynamic>>[];
    await for (final e in dir.list()) {
      if (e is! File || !e.path.endsWith('.json')) continue;
      try {
        final decoded = jsonDecode(await e.readAsString());
        if (decoded is Map<String, dynamic>) mixes.add(decoded);
        if (mixes.length >= 500) break;
      } catch (_) {}
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
    final file = File(
        '${tmp.path}/open-fader-backup.json');
    await file.writeAsString(jsonEncode(payload));
    await SharePlus.instance.share(ShareParams(files: [XFile(file.path)], subject: 'open-fader-backup.json'));
    return null;
  }

  /// Returns the imported count, or a negative error code as string.
  Future<String> importBackup() async {
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
    var count = 0;
    final dir = await _dir();
    for (final m in decoded['mixes'] as List) {
      if (validateMix(m).mix == null) continue;
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

  /// Bytes used by mixes + backup. Counts real files, not estimates.
  Future<int> storageBytes() async {
    var total = 0;
    try {
      final dir = await _dir();
      await for (final e in dir.list()) {
        if (e is File) total += await e.length();
      }
      final backup = await _backupFile();
      if (await backup.exists()) total += await backup.length();
    } catch (_) {}
    return total;
  }

  /// Clears temporary/cache files only. Mixes, recents and prefs stay.
  Future<int> clearCache() async {
    var removed = 0;
    try {
      final tmp = await getTemporaryDirectory();
      await for (final e in tmp.list()) {
        try {
          await e.delete(recursive: true);
          removed++;
        } catch (_) {}
      }
    } catch (_) {}
    return removed;
  }
}

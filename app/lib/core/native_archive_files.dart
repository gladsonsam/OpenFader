// Native ZIP bytes have their own storage. Never serialize a local scene into
// this directory or rebuild a native ZIP from its interpreted index.
import 'dart:io';
import 'dart:typed_data';
import 'dart:ui';

import 'package:file_picker/file_picker.dart';
import 'package:path_provider/path_provider.dart';
import 'package:share_plus/share_plus.dart';

class NativeArchiveFiles {
  static const maxBytes = 32 * 1024 * 1024;

  static Future<File?> latest() async {
    final documents = await getApplicationDocumentsDirectory();
    final directory = Directory('${documents.path}/native_archives');
    if (!await directory.exists()) return null;
    final files = await directory
        .list()
        .where((entry) => entry is File && entry.path.endsWith('.zip'))
        .cast<File>()
        .toList();
    files.sort((a, b) => b.path.compareTo(a.path));
    return files.firstOrNull;
  }

  static Future<Uint8List?> pick() async {
    final files = await FilePicker.pickFiles(
      type: FileType.custom,
      allowedExtensions: ['zip'],
    );
    final file = files.firstOrNull;
    if (file == null) return null;
    if ((file.lengthSync() ?? 0) > maxBytes) {
      throw StateError('Native file exceeds 32 MiB.');
    }
    final builder = BytesBuilder(copy: false);
    await for (final chunk in file.readAsByteStream()) {
      if (builder.length + chunk.length > maxBytes) {
        throw StateError('Native file exceeds 32 MiB.');
      }
      builder.add(chunk);
    }
    final bytes = builder.takeBytes();
    return bytes;
  }

  static Future<File> preserve(List<dynamic> raw, bool Function() valid) async {
    if (raw.length > maxBytes ||
        raw.any((b) => b is! int || b < 0 || b > 255)) {
      throw StateError('Invalid native archive bytes.');
    }
    final bytes = Uint8List.fromList(raw.cast<int>());
    final documents = await getApplicationDocumentsDirectory();
    final directory = Directory('${documents.path}/native_archives');
    await directory.create(recursive: true);
    if (!valid()) throw StateError('Native file context changed.');
    final stamp = DateTime.now().microsecondsSinceEpoch;
    final temporary = File('${directory.path}/show-$stamp.zip.partial');
    final target = File('${directory.path}/show-$stamp.zip');
    var created = false;
    try {
      await temporary.create(exclusive: true);
      created = true;
      await temporary.writeAsBytes(bytes, flush: true);
      final readback = await temporary.readAsBytes();
      if (readback.length != bytes.length) {
        throw StateError('Native file readback length mismatch.');
      }
      for (var i = 0; i < bytes.length; i++) {
        if (readback[i] != bytes[i]) {
          throw StateError('Native file readback mismatch.');
        }
      }
      if (!valid()) throw StateError('Native file context changed.');
      return await temporary.rename(target.path);
    } finally {
      if (created && await temporary.exists()) await temporary.delete();
    }
  }

  static Future<void> share(File file) async {
    final view = PlatformDispatcher.instance.views.first;
    final size = view.physicalSize / view.devicePixelRatio;
    final result = await SharePlus.instance.share(
      ShareParams(
        files: [XFile(file.path)],
        subject: 'OpenFader native stored-show archive (restore unverified)',
        sharePositionOrigin: Rect.fromLTWH(
          size.width / 2,
          size.height / 2,
          1,
          1,
        ),
      ),
    );
    if (result.status != ShareResultStatus.success) {
      throw StateError(
        'Export not confirmed; preserved archive remains on disk.',
      );
    }
  }
}

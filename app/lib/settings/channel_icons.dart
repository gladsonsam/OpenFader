import 'dart:convert';
import 'dart:io';
import 'dart:math';
import 'dart:typed_data';
import 'dart:ui' as ui;

import 'package:crypto/crypto.dart';
import 'package:path/path.dart' as path;
import 'package:path_provider/path_provider.dart';
import 'package:shared_preferences/shared_preferences.dart';

import '../core/channel_icon.dart';

/// Device-local default workspace and provider-layout bindings, not physical
/// desk identities or show IDs. A binding survives IP/name/session changes.
/// Each driver has one binding in this workspace; multiple desks of the same
/// provider intentionally share its layout until workspace management exists.
class ChannelIconRepository {
  static const storageKey = 'openfader.channelIcons.v1';
  final SharedPreferences _prefs;
  Map<String, dynamic> _document = {};
  String? error;
  String? _directory;
  Future<void> _queue = Future.value();
  ChannelIconRepository(this._prefs) {
    try {
      final raw = _prefs.getString(storageKey);
      if (raw == null) {
        _document = _candidate();
        return;
      }
      final data = jsonDecode(raw);
      if (data is! Map<String, dynamic> ||
          data['version'] != 1 ||
          data['workspace'] is! String ||
          data['bindings'] is! Map ||
          data['overrides'] is! Map ||
          data['images'] is! Map) {
        throw const FormatException('Unknown icon storage format');
      }
      for (final binding in (data['bindings'] as Map).values) {
        if (binding is! String || binding.isEmpty) {
          throw const FormatException('Invalid provider binding');
        }
      }
      for (final strips in (data['overrides'] as Map).values) {
        if (strips is! Map) {
          throw const FormatException('Invalid icon overrides');
        }
        for (final selection in strips.values) {
          ChannelIconSelection.fromJson(selection);
        }
      }
      _document = data;
    } catch (failure) {
      // Preserve unsupported/corrupt data. Never overwrite it with defaults.
      error = 'Saved icons are unavailable: $failure';
    }
  }

  Future<void> initialize() async {
    try {
      _directory = path.join(
          (await getApplicationSupportDirectory()).path, 'channel-icons-v1');
    } catch (_) {
      // Catalog selections remain usable if image storage is unavailable.
    }
  }

  /// Stop queued/import work before the existing explicit app-data reset.
  Future<void> prepareForReset() async {
    error = 'Icon storage is being reset. Reopen the editor afterwards.';
    await _queue;
  }

  Future<void> removeManagedImages() async {
    if (_directory == null) return;
    final directory = Directory(_directory!);
    if (await directory.exists()) await directory.delete(recursive: true);
  }

  static String _uuid() {
    final random = Random.secure();
    final bytes = List.generate(16, (_) => random.nextInt(256));
    bytes[6] = (bytes[6] & 15) | 64;
    bytes[8] = (bytes[8] & 63) | 128;
    final hex = bytes.map((v) => v.toRadixString(16).padLeft(2, '0')).join();
    return '${hex.substring(0, 8)}-${hex.substring(8, 12)}-'
        '${hex.substring(12, 16)}-${hex.substring(16, 20)}-${hex.substring(20)}';
  }

  String? scope(String driver) {
    if (error != null) return null;
    // Provisional identities become durable in the same commit as the first
    // selection. Captured editors keep the same scope during that commit.
    final binding = (_document['bindings'] as Map).putIfAbsent(driver, _uuid);
    return '${_document['workspace']}/$binding';
  }

  ChannelIconSelection selection(String driver, String strip) {
    final binding = (_document['bindings'] as Map?)?[driver];
    final raw = ((_document['overrides'] as Map?)?[binding] as Map?)?[strip];
    return raw == null
        ? const ChannelIconSelection.followProvider()
        : ChannelIconSelection.fromJson(raw);
  }

  Future<T> _serialize<T>(Future<T> Function() operation) {
    final next = _queue.then((_) => operation());
    _queue = next.then<void>((_) {}, onError: (Object _) {});
    return next;
  }

  Future<void> _commit(Map<String, dynamic> next) async {
    if (error != null) throw StateError(error!);
    try {
      if (!await _prefs.setString(storageKey, jsonEncode(next))) {
        throw StateError('The icon could not be saved on this device.');
      }
    } catch (_) {
      await _prefs.reload();
      rethrow;
    }
    _document = next;
  }

  Map<String, dynamic> _candidate() => _document.isEmpty
      ? {
          'version': 1,
          'workspace': _uuid(),
          'bindings': <String, dynamic>{},
          'overrides': <String, dynamic>{},
          'images': <String, dynamic>{},
        }
      : jsonDecode(jsonEncode(_document)) as Map<String, dynamic>;

  Future<void> save(String driver, String strip, ChannelIconSelection value,
          {required bool Function() isCurrent}) =>
      _serialize(() async {
        if (!isCurrent()) throw StateError('The icon editor target changed.');
        final next = _candidate();
        final bindings = next['bindings'] as Map;
        final binding = bindings.putIfAbsent(driver, _uuid);
        final strips = (next['overrides'] as Map)
            .putIfAbsent(binding, () => <String, dynamic>{}) as Map;
        if (value.kind == ChannelIconKind.followProvider) {
          strips.remove(strip);
        } else {
          strips[strip] = value.toJson();
        }
        await _commit(next);
      });

  List<ChannelIconSelection> get imported => [
        for (final id in (_document['images'] as Map? ?? const {}).keys)
          if (id is String && RegExp(r'^[a-f0-9]{64}$').hasMatch(id))
            ChannelIconSelection.imported(id),
      ];

  String? imagePath(String? id) {
    if (_directory == null ||
        id == null ||
        !RegExp(r'^[a-f0-9]{64}$').hasMatch(id) ||
        !(_document['images'] as Map? ?? const {}).containsKey(id)) {
      return null;
    }
    return path.join(_directory!, '$id.png');
  }

  bool imageExists(String? id) {
    final file = imagePath(id);
    return file != null && File(file).existsSync();
  }

  /// Remove one unreferenced managed asset. Every provider binding is checked
  /// under the same serialization boundary as selection saves and imports.
  Future<void> removeUnusedImage(String id,
          {required bool Function() isCurrent}) =>
      _serialize(() async {
        if (!isCurrent()) throw StateError('The icon editor target changed.');
        if (!RegExp(r'^[a-f0-9]{64}$').hasMatch(id)) {
          throw ArgumentError.value(id, 'id');
        }
        for (final strips in (_document['overrides'] as Map).values) {
          for (final raw in (strips as Map).values) {
            final selection = ChannelIconSelection.fromJson(raw);
            if (selection.kind == ChannelIconKind.imported &&
                selection.id == id) {
              throw StateError('This image is used by a saved channel icon. '
                  'Choose Automatic or another icon on that channel first.');
            }
          }
        }
        final filename = imagePath(id);
        final next = _candidate();
        (next['images'] as Map).remove(id);
        await _commit(next);
        if (filename != null && await File(filename).exists()) {
          await File(filename).delete();
        }
      });

  /// Decode only bounded raster images and store a static, aspect-preserving
  /// PNG of at most 512 x 512. Never persist picker paths or original names.
  Future<ChannelIconSelection> importImage(Uint8List bytes,
          {required bool Function() isCurrent}) =>
      _serialize(() async {
        if (error != null) throw StateError(error!);
        if (!isCurrent()) throw StateError('The icon editor target changed.');
        if (_directory == null) {
          throw StateError('Image storage is unavailable.');
        }
        if (bytes.isEmpty || bytes.length > 8 * 1024 * 1024) {
          throw StateError('Choose an image smaller than 8 MB.');
        }
        final buffer = await ui.ImmutableBuffer.fromUint8List(bytes);
        ui.ImageDescriptor? descriptor;
        ui.Codec? codec;
        ui.Image? image;
        try {
          descriptor = await ui.ImageDescriptor.encoded(buffer);
          if (descriptor.width * descriptor.height > 24 * 1024 * 1024) {
            throw StateError('Choose an image smaller than 24 megapixels.');
          }
          final scale =
              min(1.0, 512 / max(descriptor.width, descriptor.height));
          codec = await descriptor.instantiateCodec(
              targetWidth: max(1, (descriptor.width * scale).round()),
              targetHeight: max(1, (descriptor.height * scale).round()));
          image = (await codec.getNextFrame()).image;
          final png = await image.toByteData(format: ui.ImageByteFormat.png);
          if (png == null) throw StateError('This image could not be decoded.');
          if (!isCurrent()) throw StateError('The icon editor target changed.');
          final normalized = png.buffer.asUint8List();
          final id = sha256.convert(normalized).toString();
          await Directory(_directory!).create(recursive: true);
          final target = File(path.join(_directory!, '$id.png'));
          if (!await target.exists()) {
            final temporary = File('${target.path}.${_uuid()}.tmp');
            try {
              await temporary.writeAsBytes(normalized, flush: true);
              await temporary.rename(target.path);
            } finally {
              if (await temporary.exists()) await temporary.delete();
            }
          }
          if (!isCurrent()) throw StateError('The icon editor target changed.');
          final next = _candidate();
          (next['images'] as Map)[id] = {
            'width': image.width,
            'height': image.height,
          };
          await _commit(next);
          return ChannelIconSelection.imported(id);
        } finally {
          image?.dispose();
          codec?.dispose();
          descriptor?.dispose();
          buffer.dispose();
        }
      });
}

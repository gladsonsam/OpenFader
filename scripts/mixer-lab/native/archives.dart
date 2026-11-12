import 'dart:convert';
import 'dart:io';

import '../osc.dart';
import 'client.dart';

extension NativeArchiveChecks on NativeProbe {
  Future<void> verifyExportProvenance() async {
    await check(
      'Current-mix archive descriptor excludes stored-scene slot provenance',
      () async {
        final current = await refresh();
        final archive = current.archive;
        if (archive == null)
          throw Blocked('Native archive operations unavailable');
        final scene = archive.kinds.singleWhere((kind) => kind.id == 'scene');
        require(
          scene.label.toLowerCase().contains('current mix') &&
              scene.slotMin == null &&
              scene.slotMax == null,
          'Current mix export still advertises a saved-scene slot',
        );
        final result = await request({
          'op': archive.exportOp,
          'kind': 'scene',
          'slot': 99,
        }, expected: current.guard);
        require(
          result['status'] == 'rejected' &&
              '${result['message']}'.contains('reopen Export current mix'),
          'Legacy saved-scene export intent was silently accepted: $result',
        );
      },
    );
  }

  Future<void> verifyArchives() async {
    for (final kind
        in (options['--only'] == 'export-provenance'
            ? ['scene']
            : ['routing', 'scene'])) {
      await check('$kind native archive export through app contract', () async {
        final result = await action({'op': 'export_archive', 'kind': kind});
        require(result['archive'] is Map, 'Export returned no archive');
        final exported = result['archive'] as Map;
        require(
          exported.containsKey('slot') &&
              exported['slot'] == null &&
              exported['source'] ==
                  (kind == 'scene' ? 'current_mix' : 'current_routing'),
          'Export claims false saved-scene provenance',
        );
        await File('$reportPath.$kind-archive.json')
            .writeAsString(jsonEncode(result['archive']));
        if (result['status'] != 'confirmed')
          throw Blocked('Partial native export: ${result['message']}');
      });
      if (options['--only'] == 'export-provenance') continue;
      final path = kind == 'scene' ? '/ch/32/config' : '/outputs/main/16';
      Map<String, dynamic>? archive;
      await check('$kind local native archive inspect', () async {
        archive = {
          'schema': 'openfader-x32-native/1',
          'kind': kind,
          'nodes': [
            {'path': path, 'text': await _readNode(osc, path)},
          ],
        };
        final result = await action({
          'op': 'inspect_archive',
          'archive': archive,
        });
        require(
          result['status'] == 'confirmed',
          'Archive inspection not confirmed',
        );
      });
      await check(
        '$kind native archive import and rollback independently preserve node text',
        () async {
          if (archive == null)
            throw Blocked('No independently captured archive node');
          final before = await _readNode(osc, path);
          final leaf = kind == 'scene'
              ? '/ch/32/config/name'
              : '/outputs/main/16/invert';
          final original = await osc.read(leaf);
          originals.putIfAbsent(leaf, () => original);
          final desired = original is String
              ? '$runTag-N'
              : (original == 0 ? 1 : 0);
          await osc.write(leaf, desired);
          await osc.agrees(leaf, desired);
          final changed = await _readNode(osc, path);
          require(
            changed != before,
            'Independent node did not capture staged change',
          );
          final result = await action({
            'op': 'import_archive',
            'archive': archive,
          });
          require(
            result['backup'] is Map && result['progress'] is Map,
            'Import omitted rollback/progress evidence',
          );
          require(
            (result['progress'] as Map)['failed'] is List &&
                ((result['progress'] as Map)['failed'] as List).isEmpty,
            'Native archive import reports failed paths',
          );
          require(
            await _readNode(osc, path) == before,
            'Native import changed independently captured node values',
          );
          final rollback = await action({
            'op': 'import_archive',
            'archive': result['backup'],
          });
          require(rollback['backup'] is Map, 'Rollback omitted its own backup');
          require(
            await _readNode(osc, path) == changed,
            'Native rollback did not restore the staged node change',
          );
          await osc.write(leaf, original);
          await osc.agrees(leaf, original);
          require(
            await _readNode(osc, path) == before,
            'Archive fixture final restoration mismatch',
          );
        },
      );
    }
  }
}

Future<String> _readNode(NativeOsc osc, String path) async {
  final socket = await RawDatagramSocket.bind(InternetAddress.loopbackIPv4, 0);
  try {
    final bytes = osc.packet('/node', path.substring(1));
    socket.send(bytes, InternetAddress.loopbackIPv4, 10023);
    await for (final event in socket.timeout(const Duration(seconds: 2))) {
      if (event != RawSocketEvent.read) continue;
      final datagram = socket.receive();
      if (datagram == null ||
          !datagram.address.isLoopback ||
          datagram.port != 10023)
        continue;
      final bytes = datagram.data;
      final end = bytes.indexOf(0);
      if (end < 0 || utf8.decode(bytes.sublist(0, end)) != 'node') continue;
      final tagAt = ((end + 4) ~/ 4) * 4;
      final tagEnd = bytes.indexOf(0, tagAt);
      if (tagEnd < 0 || utf8.decode(bytes.sublist(tagAt, tagEnd)) != ',s')
        continue;
      final at = ((tagEnd + 4) ~/ 4) * 4;
      final textEnd = bytes.indexOf(0, at);
      if (textEnd < 0) continue;
      final text = utf8.decode(bytes.sublist(at, textEnd));
      if (text == path ||
          text.startsWith('$path ') ||
          text.startsWith('$path\n'))
        return text;
    }
    throw StateError('Node socket closed for $path');
  } finally {
    socket.close();
  }
}

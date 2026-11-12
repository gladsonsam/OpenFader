import 'dart:convert';
import 'dart:io';

import '../../../app/lib/engine_protocol.dart';
import '../osc.dart';
import 'client.dart';

extension X32Groups on X32Probe {
  Future<void> groupsPass() async {
    await check('Invalid input matrix send and DCA EQ writes reject', () async {
      await command(Cmds.param(0, 'send17_on', true), reject: true);
      final dca =
          families.singleWhere((f) => f['key'] == 'dca')['first'] as int;
      await command(Cmds.param(dca, 'eq1_gain', 0), reject: true);
    });
    for (final family in families.where(
      (f) => ['ch', 'auxin', 'fxrtn', 'bus', 'mtx'].contains(f['key']),
    )) {
      final first = family['first'] as int;
      final leaf = {
        'ch': 'chlink',
        'auxin': 'auxlink',
        'fxrtn': 'fxlink',
        'bus': 'buslink',
        'mtx': 'mtxlink',
      }[family['key']]!;
      for (final rawPair in caps!['stereo_pairs'] as List) {
        final pair = (rawPair as List).cast<int>();
        if (pair[0] < first || pair[0] >= first + (family['count'] as int))
          continue;
        final local = pair[0] - first + 1;
        final key = (family['strip_keys'] as List)[local - 1] as String;
        await check(
          'Stereo pair $key links/unlinks with external updates on both members',
          () async {
            final path = '/config/$leaf/$local-${local + 1}';
            await remember(path);
            for (final linked in [true, false]) {
              await command(Cmds.param(pair[0], 'link', linked));
              await native.agrees(path, linked ? 1 : 0);
            }
            final sequenceBeforeExternal = sequence;
            await native.write(path, 1);
            await native.agrees(path, 1);
            await until(
              () => pair.every(
                (ch) =>
                    (valueSequence['$ch/link'] ?? 0) > sequenceBeforeExternal &&
                    values['$ch/link'] == true,
              ),
              'External stereo link observation on both $key members',
            );
          },
        );
      }
    }
    await check('Partner lock rejects a stereo edit without changing the native pair', () async {
      final pair = (caps!['stereo_pairs'] as List)
          .cast<List>()
          .first
          .cast<int>();
      const path = '/config/chlink/1-2';
      final original = await remember(path);
      final other = await WebSocket.connect(uri.toString());
      var otherInfo = 0, otherHello = 0;
      final otherErrors = <Map>[];
      final otherSubscription = other.listen((raw) {
        if (raw is! String) return;
        final event = jsonDecode(raw) as Map;
        if (event['type'] == 'info') otherInfo++;
        if (event['type'] == 'hello') otherHello++;
        if (event['type'] == 'error') otherErrors.add(event);
      });
      Future<void> otherCommand(String value) async {
        final marker = otherHello;
        final errorMarker = otherErrors.length;
        other.add(value);
        other.add(Cmds.hello(false, true));
        await until(
          () => otherHello > marker,
          'Partner-client command completion',
        );
        require(
          otherErrors.length == errorMarker,
          'Partner-client command error: ${otherErrors.skip(errorMarker).toList()}',
        );
      }

      var locked = false;
      try {
        // Drain the initial acknowledgement before starting monotonic barriers.
        other.add(Cmds.hello(false, true));
        await until(
          () => otherInfo > 0 && otherHello > 0,
          'Partner-client attachment and initial hello',
        );
        await otherCommand(Cmds.lock(pair[1]));
        locked = true;
        final rejected = await command(
          Cmds.param(pair[0], 'link', original == 0),
          reject: true,
        );
        require(
          rejected.any((e) => '${e['message']}'.toLowerCase().contains('lock')),
          'Link rejected for a reason other than the partner lock: $rejected',
        );
        await native.agrees(path, original);
      } finally {
        if (locked) await otherCommand(Cmds.unlock(pair[1]));
        await other.close();
        await otherSubscription.cancel();
      }
    });
    final groupCount = caps!['groups']['mute_groups'] as int;
    for (var g = 0; g < groupCount; g++)
      await check('Mute-group master ${g + 1} native readback', () async {
        final path = '/config/mute/${g + 1}';
        final old = await remember(path);
        final desired = old == 0;
        await command(Cmds.muteGroup(g, desired));
        await native.agrees(path, desired ? 1 : 0);
      });
  }
}

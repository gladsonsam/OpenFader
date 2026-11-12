import 'dart:convert';

import '../../../app/lib/engine_protocol.dart';
import '../osc.dart';
import 'client.dart';

extension X32Lifecycle on X32Probe {
  Future<void> scenesAndMeters() async {
    await check('Local scene save, scoped recall, rename and delete', () async {
      const number = 999;
      if ((latest['scenes']?['scenes'] as List? ?? []).any(
        (s) => s['n'] == number,
      ))
        throw Blocked('Reserved lab scene slot already occupied');
      const path = '/ch/01/mix/fader';
      await remember(path);
      await command(Cmds.fader(0, 383.0 / 1023.0, 'ch/01'));
      try {
        await command(Cmds.sceneSave(number, 'X32 laboratory'));
      } catch (error) {
        final absent = RegExp(
          r'snapshot capture incomplete: strip (70|71) (dca[1-8]_assign|_mg[1-6]) unavailable',
        ).firstMatch('$error');
        if (absent != null) {
          final nativeRoot = absent[1] == '70' ? '/main/st' : '/main/m';
          final group = absent[2]!.startsWith('dca') ? 'dca' : 'mute';
          final missingPath = '$nativeRoot/grp/$group';
          if (unavailableNativeReads.containsKey(missingPath)) {
            throw Blocked(
              'Complete local scene capture correctly fails closed because documented native $missingPath is independently absent in pinned Sidecar1330613; no partial scene was accepted. Primary protocol4.09-02 retains main DCA/mute groups. Exact capture failure: $error',
            );
          }
        }
        rethrow;
      }
      await command(Cmds.fader(0, 639.0 / 1023.0, 'ch/01'));
      await command(
        jsonEncode({
          'cmd': 'scene_recall',
          'n': number,
          'scope': {
            'strips': [0],
            'groups': ['fader_pan'],
            'globals': [],
          },
        }),
      );
      await native.agrees(path, 383.0 / 1023.0);
      await command(Cmds.sceneRename(number, 'X32 laboratory renamed'));
      await command(Cmds.sceneDelete(number));
    });
    await check('Meter stream has complete finite provider frames', () async {
      await until(() => meters > 2, 'X32 meter subscription');
      final levels = latest['meters']!['levels'] as List;
      require(
        levels.length == 80,
        'Meter topology differs from descriptors: ${levels.length}',
      );
      require(
        levels.every((v) => v is num && v.isFinite),
        'Non-finite meter values',
      );
    });
  }

  Future<void> reconnectPass() async {
    await check(
      'Disconnect disables writes, reconnect starts a fresh observed epoch',
      () async {
        final oldEpoch = latest['sync_epoch']?['epoch'];
        require(oldEpoch is int, 'Initial session epoch not observed');
        const path = '/ch/01/mix/fader';
        final original = await remember(path);
        await command(Cmds.disconnect());
        require(
          latest['link']?['state'] == 'disconnected',
          'Disconnect did not publish a disconnected link',
        );
        final disconnectedEpoch = latest['sync_epoch']?['epoch'];
        require(
          disconnectedEpoch != oldEpoch,
          'Disconnect did not invalidate the previous session epoch',
        );
        final rejection = await command(Cmds.fader(0, 0.5), reject: true);
        require(
          rejection.any((e) => '${e['message']}'.contains('not connected')),
          'Disconnected write rejected for an unrelated reason: $rejection',
        );
        await native.agrees(path, original);
        socket.add(Cmds.connect('x32', '127.0.0.1'));
        await barrier();
        require(
          latest['sync_epoch']?['epoch'] != disconnectedEpoch,
          'Reconnect reused the disconnected session epoch',
        );
        require(
          latest['link']?['state'] == 'connected',
          'Reconnect did not publish a connected link',
        );
        // Read fresh identity explicitly; the hello barrier proves command
        // completion, not the presence or identity of a physical session.
        await command(Cmds.info());
        require(
          latest['info']?['driver'] == 'x32' &&
              latest['info']?['model'] == 'X32',
          'Reconnected identity not observed',
        );
        await change(
          '/ch/01/mix/fader',
          255.0 / 1023.0,
          Cmds.fader(0, 255.0 / 1023.0, 'ch/01'),
        );
      },
    );
  }
}

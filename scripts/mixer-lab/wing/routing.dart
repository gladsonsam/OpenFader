import '../osc.dart';
import '../../../app/lib/engine_protocol.dart';
import 'client.dart';

extension WingRouting on WingProbe {
  Future<void> routingPass() async {
    for (final family in families.where(
      (f) => f['key'] == 'ch' || f['key'] == 'aux',
    )) {
      final ch = family['first'] as int, root = '/${family['key']}/1/in/conn';
      for (final target in ['', 'alt']) {
        await check('$root $target LCL input patch', () async {
          final prefix = target == 'alt' ? 'alt' : '';
          await remember('$root/${prefix}grp');
          await rememberInteger('$root/${prefix}in', 1, 64);
          try {
            await command(Cmds.patchSet(ch, 'LCL', 1, target: target));
          } catch (_) {
            final group = await native.read('$root/${prefix}grp');
            final index = await readInteger('$root/${prefix}in', 1, 64);
            results.add({
              'check': 'Routing readback after failed $root $target patch',
              'status': 'observation',
              'group': group,
              'index': index,
            });
            if (group == 'A' && index == 1) {
              throw Blocked(
                'Pinned Sidecar aliases LCL 1 to A 1 while applying the second field of the node update; exact group identity was not preserved',
              );
            }
            rethrow;
          }
          await native.agrees('$root/${prefix}grp', 'LCL');
          await agreesInteger('$root/${prefix}in', 1, 1, 64);
        });
      }
      for (final target in ['', 'alt']) {
        await check('$root $target AES50 A input patch', () async {
          final prefix = target == 'alt' ? 'alt' : '';
          await remember('$root/${prefix}grp');
          await rememberInteger('$root/${prefix}in', 1, 64);
          await command(Cmds.patchSet(ch, 'A', 1, target: target));
          await native.agrees('$root/${prefix}grp', 'A');
          await agreesInteger('$root/${prefix}in', 1, 1, 64);
        });
      }
    }
    await check('Stale source mode guard rejects without mutation', () async {
      const path = '/io/in/A/1/mode';
      final old = await remember(path);
      await remember('/ch/1/in/set/altsrc');
      await command(Cmds.param(0, 'source_select', 0.0));
      await command(Cmds.patchSet(0, 'A', 1));
      await command(
        Cmds.param(0, 'wing:source_mode', 'USB/1:M>ST'),
        reject: true,
      );
      await native.agrees(path, old);
    });
    await check('Source-mode edit pinned to AES50 A source', () async {
      const path = '/io/in/A/1/mode';
      final old = await remember(path);
      await command(Cmds.patchSet(0, 'A', 1));
      final target = old == 'M' ? 'ST' : 'M';
      await command(Cmds.param(0, 'wing:source_mode', 'A/1:$old>$target'));
      await native.agrees(path, target);
    });
    await check('AES50 A half-dB gain granularity on fixture', () async {
      const path = '/io/in/A/1/g';
      await remember(path);
      await native.write(path, 0.0);
      await agreesEventually(path, 0.0);
      await native.write(path, 6.0);
      await agreesEventually(path, 5.0);
      throw Blocked(
        'Independent OSC write of valid AES50 A gain 6 dB reads back 5 dB: pinned Sidecar aliases A/1 to its 2.5 dB-step local source; the 0.5 dB native domain remains unverified',
      );
    });
    await check('AES50 A preamp gain with guarded source', () async {
      await command(Cmds.patchSet(0, 'A', 1));
      final path = '/io/in/A/1/g';
      try {
        await remember(path);
      } catch (e) {
        throw Blocked('AES50 A preamp read unavailable: $e');
      }
      await command(Cmds.param(0, 'source_select', 0.0));
      socket.add(Cmds.focus([0]));
      await until(
        () =>
            values['0/source_a'] == 3001.0 && values['0/source_select'] == 0.0,
        'AES50 A preamp source context',
      );
      final context = <String, Object>{
        'source_a': values['0/source_a']!,
        'source_select': values['0/source_select']!,
      };
      await native.write(path, 0.0);
      await agreesEventually(path, 0.0);
      const desired = 5.0;
      await command(Cmds.paramGuarded(0, 'gain', desired, context));
      await agreesEventually(path, desired);
      await command(
        Cmds.paramGuarded(0, 'gain', desired == 0.0 ? 6.0 : 0.0, {
          ...context,
          'source_a': 2001.0,
        }),
        reject: true,
      );
      await native.agrees(path, desired);
    });
    await check('AES50 B half-dB guarded preamp and stale context', () async {
      await command(Cmds.patchSet(0, 'B', 1));
      const path = '/io/in/B/1/g';
      await remember(path);
      await native.write(path, 6.0);
      await agreesEventually(path, 6.0);
      socket.add(Cmds.focus([0]));
      await until(
        () =>
            values['0/source_a'] == 4001.0 && values['0/source_select'] == 0.0,
        'AES50 B preamp source context',
      );
      final context = <String, Object>{
        'source_a': values['0/source_a']!,
        'source_select': values['0/source_select']!,
      };
      await command(Cmds.paramGuarded(0, 'gain', 6.5, context));
      await agreesEventually(path, 6.5);
      await command(
        Cmds.paramGuarded(0, 'gain', 6.0, {...context, 'source_a': 3001.0}),
        reject: true,
      );
      await native.agrees(path, 6.5);
    });
    results.add({
      'check': 'Unmapped input source groups and physical preamp',
      'status': 'blocked',
      'reason': 'Sidecar stores some source groups rather than routing them; OSC does not prove physical headamp behavior.',
    });
  }
}

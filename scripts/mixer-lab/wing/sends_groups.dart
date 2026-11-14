import '../../../app/lib/engine_protocol.dart';
import '../osc.dart';
import 'client.dart';

extension WingSendsGroups on WingProbe {
  Future<void> sendsGroupsPass() async {
    for (final entry in [
      (1, 'send/1'),
      (16, 'send/16'),
      (17, 'send/MX1'),
      (24, 'send/MX8'),
      (25, 'main/1'),
      (28, 'main/4'),
    ]) {
      await check('Channel send ${entry.$1} native on and level', () async {
        final root = '/ch/1/${entry.$2}';
        final on = await remember('$root/on');
        await remember('$root/lvl');
        await command(Cmds.param(0, 'send${entry.$1}_on', on == 0));
        await native.agrees('$root/on', on == 0 ? 1 : 0);
        await command(Cmds.param(0, 'send${entry.$1}_level', -30.0));
        await native.agrees('$root/lvl', -30.0);
      });
    }
    await check('DCA tag edits preserve independent unrelated tags', () async {
      const path = '/ch/1/tags';
      await remember(path);
      await native.write(path, '#D2,#M3,lab-note');
      final baseline = (await native.read(path) as String).split(',').toSet();
      require(
        baseline.containsAll({'#D2', '#M3', 'lab-note'}) &&
            !baseline.contains('#D1'),
        'Independent fixture did not retain baseline tags: $baseline',
      );
      await command(Cmds.param(0, 'dca1_assign', true));
      final actual = (await native.read(path) as String).split(',').toSet();
      require(
        actual.containsAll({...baseline, '#D1'}),
        'Tag edit lost unrelated values: $actual',
      );
    });
    for (final group in [1, 16]) {
      await check('DCA membership $group preserves other tags', () async {
        const path = '/ch/1/tags';
        await remember(path);
        final old = await native.read(path) as String;
        final parts = old.split(',').where((v) => v.isNotEmpty).toSet();
        final tag = '#D$group', desired = !parts.contains(tag);
        await command(Cmds.param(0, 'dca${group}_assign', desired));
        final actual = (await native.read(path) as String)
            .split(',')
            .where((v) => v.isNotEmpty)
            .toSet();
        require(
          actual.contains(tag) == desired &&
              actual.containsAll(parts.difference({tag})),
          'Tag edit lost unrelated memberships: $actual',
        );
      });
    }
    for (final group in [1, 8]) {
      await check('Mute membership $group native tags', () async {
        const path = '/ch/1/tags';
        final tags = await remember(path) as String;
        final current = (await native.read(path) as String)
            .split(',')
            .contains('#M$group');
        await command(Cmds.param(0, '_mg$group', !current));
        final actual = (await native.read(path) as String).split(',');
        require(
          actual.contains('#M$group') != current,
          'Mute group not updated; original $tags',
        );
      });
    }
    await check('External send update reaches fresh engine state', () async {
      const path = '/ch/1/send/1/on';
      await remember(path);
      await native.write(path, 1);
      await native.agrees(path, 1);
      socket.add(Cmds.focus([0]));
      await until(
        () => values['0/send1_on'] == true,
        'Initial send observation',
      );
      final beforeOff = sequence;
      await native.write(path, 0);
      await native.agrees(path, 0);
      socket.add(Cmds.focus([0]));
      await until(
        () =>
            (observed['0/send1_on'] ?? 0) > beforeOff &&
            values['0/send1_on'] == false,
        'External send off observation',
      );
      final start = sequence;
      await native.write(path, 1);
      await native.agrees(path, 1);
      socket.add(Cmds.focus([0]));
      await until(
        () =>
            (observed['0/send1_on'] ?? 0) > start &&
            values['0/send1_on'] == true,
        'External send update',
      );
    });
  }
}

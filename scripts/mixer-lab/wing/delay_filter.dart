import '../../../app/lib/engine_protocol.dart';
import '../osc.dart';
import 'client.dart';

extension WingDelayFilter on WingProbe {
  Future<void> delayFilterPass() async {
    for (final f in families.where((f) => f['key'] != 'dca')) {
      final key = f['key'] as String, ch = f['first'] as int;
      final base = key == 'ch' || key == 'aux'
          ? '/$key/1/in/set/dly'
          : '/$key/1/dly/';
      final mode = '${base}mode';
      final value = key == 'ch' || key == 'aux' ? base : '${base}dly';
      await check(
        '$key native tagged delay and stale-unit rejection',
        () async {
          await remember(mode);
          await remember(value);
          for (final unit in [
            (0.0, 'M'),
            (1.0, 'FT'),
            (2.0, 'MS'),
            (3.0, 'SMP'),
          ]) {
            await command(Cmds.param(ch, 'wing:delay_unit', unit.$1));
            await native.agrees(mode, unit.$2);
            await command(Cmds.param(ch, 'wing:delay_value', '${unit.$2}:30'));
            await native.agrees(value, 30.0);
          }
          await command(
            Cmds.param(ch, 'wing:delay_value', 'MS:30'),
            reject: true,
          );
          await native.agrees(value, 30.0);
        },
      );
    }
    await check(
      'Channel filter models and guarded native parameters',
      () async {
        const model = '/ch/1/flt/mdl';
        final original = await remember(model);
        const originalLeaves = {
          'TILT': ['tilt'],
          'MAX': ['low', 'proc'],
          'AP1': ['f'],
          'AP2': ['f', 'q'],
        };
        if (!originalLeaves.containsKey(original)) {
          throw Blocked('Unknown original filter model $original');
        }
        for (final leaf in originalLeaves[original]!) {
          await remember('/ch/1/flt/$leaf');
        }
        for (final fixture in [
          (0.0, 'TILT', 'wing:filter_tilt', 'tilt', 3.0),
          (1.0, 'MAX', 'wing:filter_low', 'low', 100.0),
          (2.0, 'AP1', 'wing:filter_ap1_freq', 'f', 100.0),
          (3.0, 'AP2', 'wing:filter_ap2_q', 'q', 0.5),
        ]) {
          await command(Cmds.param(0, 'wing:filter_model', fixture.$1));
          await native.agrees(model, fixture.$2);
          await command(Cmds.param(0, fixture.$3, fixture.$5));
          await native.agrees('/ch/1/flt/${fixture.$4}', fixture.$5);
        }
        await command(Cmds.param(0, 'wing:filter_tilt', 3.0), reject: true);
      },
    );
  }
}

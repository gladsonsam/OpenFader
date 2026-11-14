import '../../../app/lib/engine_protocol.dart';
import '../../../app/lib/core/channel_views.dart' show parameterPresentation;

import 'dart:convert';

import '../osc.dart';
import 'client.dart';

extension WingProcessing on WingProbe {
  Future<void> processingPass() async {
    await check('WING native family topology', () async {
      const counts = {
        'ch': 40,
        'aux': 8,
        'bus': 16,
        'main': 4,
        'mtx': 8,
        'dca': 16,
      };
      const first = {
        'ch': 0,
        'aux': 40,
        'bus': 48,
        'main': 64,
        'mtx': 68,
        'dca': 76,
      };
      require(families.length == counts.length, 'Missing or extra family');
      for (final f in families) {
        require(
          f['count'] == counts[f['key']] && f['first'] == first[f['key']],
          'Unexpected family $f',
        );
      }
    });
    await check(
      'EQ6 frequency kind ranges resolve through the app contract',
      () async {
        final rows = (caps!['params'] as List).cast<Map>();
        final metadata = Map<String, dynamic>.from(
          rows.singleWhere((row) => row['id'] == 'eq6_freq'),
        );
        require(
          metadata['min'] == 20 && metadata['max'] == 20000,
          'Global EQ6 range must remain the union 20..20000',
        );
        final withoutRanges = Map<String, dynamic>.from(metadata);
        withoutRanges['presentations'] = [
          for (final raw in metadata['presentations'] as List)
            if (raw is Map)
              Map<String, dynamic>.from(raw)..remove('kind_ranges')
            else
              raw,
        ];
        for (final family in ['ch', 'aux', 'bus', 'main', 'mtx']) {
          final kind =
              families.singleWhere((f) => f['key'] == family)['kind'] as String;
          final resolved = parameterPresentation(
            metadata,
            (_) => 0.0,
            stripKind: kind,
          )!;
          final baseline = parameterPresentation(
            withoutRanges,
            (_) => 0.0,
            stripKind: kind,
          )!;
          final minimum = family == 'ch' || family == 'aux' ? 50 : 20;
          require(
            resolved['min'] == minimum && resolved['max'] == 20000,
            '$family EQ6 presentation has wrong domain: $resolved',
          );
          require(
            jsonEncode(resolved['choices']) == jsonEncode(baseline['choices']),
            '$family range projection erased existing choices',
          );
        }
      },
    );
    await check(
      'EQ6 high-band rejection and output mid-band boundary',
      () async {
        final bus =
            families.singleWhere((f) => f['key'] == 'bus')['first'] as int;
        const input = '/ch/1/eq/hf', output = '/bus/1/eq/5f';
        await native.agrees('/ch/1/eq/mdl', 'STD');
        await native.agrees('/bus/1/eq/mdl', 'STD');
        final before = await remember(input);
        await remember(output);
        await command(Cmds.param(0, 'eq6_freq', 20.0), reject: true);
        await native.agrees(input, before);
        await native.write(output, 1000.0);
        final staged = await native.read(output);
        require(
          staged is num && staged > 50,
          'Unable to stage distinct output EQ value',
        );
        await command(Cmds.param(bus, 'eq6_freq', 20.0));
        await agreesEventually(output, 20.0);
      },
    );
    for (final family in families) {
      final first = family['first'] as int, count = family['count'] as int;
      final name = family['key'] as String;
      for (final index in [0, count - 1]) {
        final ch = first + index, root = '/$name/${index + 1}';
        await check('$root fader and mute boundary command/readback', () async {
          await remember('$root/fdr');
          await native.write('$root/fdr', -36.0);
          await native.agrees('$root/fdr', -36.0);
          await change('$root/fdr', -60.0, Cmds.fader(ch, 84.0 / 154));
          final old = await remember('$root/mute');
          final desired = old == 0;
          await command(Cmds.mute(ch, desired));
          await native.agrees('$root/mute', desired ? 1 : 0);
        });
        final color = await captureColor('$root/col');
        if (color == 0) {
          await check('$root initial native colour domain', () async {
            throw Blocked(
              'Fresh Sidecar colour 0 is outside its live native domain; this original requires fixture reset.',
            );
          });
          await initializeColor('$root/col');
        }
        await check(
          '$root color and name boundary command/readback on valid-domain disposable fixture',
          () async {
            await native.write('$root/col', 1);
            await agreesColor('$root/col', 1);
            await command(Cmds.param(ch, 'color', 12.0));
            await agreesColor('$root/col', 12);
            await remember('$root/name');
            await native.write('$root/name', 'BEFORE');
            await native.agrees('$root/name', 'BEFORE');
            await command(Cmds.param(ch, 'name', 'OF-LAB'));
            await native.agrees('$root/name', 'OF-LAB');
          },
        );
      }
    }
    // Engineering values are independent WING protocol fixtures (v3.0.5-1).
    for (final p in [
      (id: 'trim', leaf: 'in/set/trim', value: -6.0),
      (id: 'pan', leaf: 'pan', value: 50.0),
      (id: 'gate_thr', leaf: 'gate/thr', value: -40.0),
      (id: 'gate_att', leaf: 'gate/att', value: 30.0),
      (id: 'comp_thr', leaf: 'dyn/thr', value: -30.0),
      (id: 'comp_att', leaf: 'dyn/att', value: 30.0),
      (id: 'eq1_gain', leaf: 'eq/lg', value: 7.5),
    ]) {
      await check('Channel 1 ${p.id} engineering readback', () async {
        final path = '/ch/1/${p.leaf}';
        await remember(path);
        await native.write(path, 0.0);
        await native.agrees(path, 0.0);
        await command(Cmds.param(0, p.id, p.value));
        await native.agrees(path, p.value);
      });
    }
    // Midpoints on the frozen 3.0.5 log scales (961 EQ and 181 Q steps).
    for (final p in [
      (id: 'eq1_q', leaf: 'eq/lq', value: 2.1023796, baseline: 10.0),
      (id: 'hpf_freq', leaf: 'flt/lcf', value: 200.0, baseline: 20.0),
    ]) {
      await check('Channel 1 ${p.id} interior logarithmic value', () async {
        final path = '/ch/1/${p.leaf}';
        await remember(path);
        await native.write(path, p.baseline);
        await native.agrees(path, p.baseline);
        await command(Cmds.param(0, p.id, p.value));
        final actual = await native.read(path);
        require(
          actual is num && (actual - p.value).abs() < 0.001,
          '$path expected ${p.value}, observed $actual',
        );
      });
    }
    await check('Unsupported DCA EQ and bus self-send reject', () async {
      final dca =
          families.singleWhere((f) => f['key'] == 'dca')['first'] as int;
      final bus =
          families.singleWhere((f) => f['key'] == 'bus')['first'] as int;
      await command(Cmds.param(dca, 'eq1_gain', 1.0), reject: true);
      await command(Cmds.param(bus, 'send1_on', true), reject: true);
    });
    await check('External channel fader reaches fresh engine state', () async {
      const path = '/ch/1/fdr';
      await remember(path);
      await native.write(path, -60.0);
      await native.agrees(path, -60.0);
      final start = sequence;
      await native.write(path, -36.0);
      await native.agrees(path, -36.0);
      await until(
        () =>
            (observed['0/fader'] ?? 0) > start &&
            same(values['0/fader'], 108.0 / 154),
        'External WING fader',
      );
    });
  }
}

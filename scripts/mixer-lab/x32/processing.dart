import 'dart:convert';

import '../../../app/lib/engine_protocol.dart';
import '../../../app/lib/core/channel_views.dart'
    show parameterPresentation, parameterChoices;
import '../osc.dart';
import 'client.dart';
import 'fixtures.dart';

extension X32Processing on X32Probe {
  Future<void> processingPass() async {
    await check('Complete X32 identity and native topology', () async {
      require(latest['info']?['model'] == 'X32', 'Unexpected fixture identity');
      require(
        families.fold<int>(0, (n, f) => n + (f['count'] as int)) == 80,
        'X32 topology missing strips',
      );
      require(
        (caps!['stereo_pairs'] as List).length == 35,
        'Missing stereo pair domains',
      );
      require(
        (caps!['fx']['types'] as List).length == 61,
        'Missing FX catalogue',
      );
    });
    await check(
      'Rapid sequential writes complete within normal response deadline',
      () async {
        final first = families.singleWhere((f) => f['key'] == 'ch');
        final ch = first['first'] as int;
        final path =
            '${rootFor((first['strip_keys'] as List).first as String)}/mix/on';
        final original = await remember(path);
        final start = errors.length;
        final watch = Stopwatch()..start();
        for (final muted in [true, false, true, false, true, false])
          socket.add(Cmds.mute(ch, muted));
        await barrier();
        require(
          watch.elapsedMilliseconds < 5000,
          'Six sequential edits took ${watch.elapsedMilliseconds}ms',
        );
        require(errors.length == start, 'Burst produced errors');
        await native.agrees(path, 1);
        await native.write(path, original);
      },
    );
    const readOnly = {
      'source',
      'x32:preamp_state',
      'x32:eq_bands',
      'x32:eq_type_count',
      'x32:automix_gain',
      'gate_gr',
      'comp_gr',
    };
    for (final family in families) {
      final familyKey = family['key'] as String;
      final ch = family['first'] as int;
      final key = (family['strip_keys'] as List).first as String;
      final root = rootFor(key);
      final scopes = (family['params_scope'] as List).cast<String>();
      for (final p in metadata) {
        final id = p['id'] as String;
        if (!(p['scopes'] as List).any(scopes.contains)) continue;
        if (!applicable(id, familyKey)) {
          results.add({
            'check': '$key/$id',
            'status': 'not_applicable',
            'reason': 'Native X32 family has no leaf; generic kind-level metadata alone is insufficient',
          });
          continue;
        }
        if (readOnly.contains(id)) {
          results.add({
            'check': '$key/$id',
            'status': 'read_only',
            'reason':
                'Derived/telemetry field; write pass-through is not applicable',
          });
          continue;
        }
        if (id == 'link') {
          results.add({
            'check': '$key/$id',
            'status': 'covered_by_stereo_scenario',
            'reason': 'All advertised native pairs are verified in the dedicated link and external-partner scenarios',
          });
          continue;
        }
        if (id == 'gain' || id == 'phantom') {
          results.add({
            'check': '$key/$id',
            'status': 'covered_by_guarded_preamp_scenario',
            'reason': 'Never sent as an unguarded param; see dedicated source-aware preamp scenario',
          });
          continue;
        }
        if (RegExp(r'^eq[1-6]_type$').hasMatch(id) &&
            ['mtx', 'main', 'mainm'].contains(familyKey)) {
          await check(
            '$key/$id retains all14 hardware-native descriptors',
            () async {
              final presentation = parameterPresentation(
                Map<String, dynamic>.from(p),
                (name) => name == 'x32:eq_type_count' ? 14.0 : null,
              );
              require(
                parameterChoices(presentation).length == 14,
                'Native output EQ domain was incorrectly narrowed',
              );
            },
          );
          results.add({
            'check': '$key/$id native options6..13',
            'status': 'blocked',
            'reason': 'Hardware protocol4.09-02 documents0..13 (primary author: https://sites.google.com/site/patrickmaillot/x32); pinned Sidecar1330613 rust/crates/livedaw-x32/src/map.rs uses livedaw_dsp::x32::EQ_TYPES with only0..5 for every family. Common0..5 tested separately; no native domain narrowed.',
          });
        }
        await check('$key/$id command and native readback', () async {
          var candidate = sampleFor(id, p, familyKey, 0);
          if (candidate == null)
            throw Blocked('No independent native fixture mapping');
          final path = '$root/${candidate.leaf}';
          Object original;
          try {
            original = await remember(path);
          } catch (e) {
            throw Blocked(
              'Independent emulator leaf unavailable before write: $path ($e)',
            );
          }
          candidate = sampleFor(id, p, familyKey, original)!;
          await command(Cmds.param(ch, id, candidate.value));
          await native.agrees(path, candidate.wire);
        });
      }
    }
    await check(
      'Complete families: final strip fader and mute boundaries',
      () async {
        for (final family in families) {
          final count = family['count'] as int;
          final ch = (family['first'] as int) + count - 1;
          final key = (family['strip_keys'] as List).last as String;
          final root = rootFor(key);
          final dca = family['key'] == 'dca';
          await change(
            '$root/${dca ? 'fader' : 'mix/fader'}',
            511.0 / 1023.0,
            Cmds.fader(ch, 511.0 / 1023.0, key),
          );
          await change(
            '$root/${dca ? 'on' : 'mix/on'}',
            0,
            Cmds.mute(ch, true, key),
          );
        }
      },
    );
    for (final family in families.where(
      (f) => f['key'] == 'ch' || f['key'] == 'auxin',
    )) {
      final ch = family['first'] as int;
      final key = (family['strip_keys'] as List).first as String;
      await check('Source-aware guarded preamp on $key', () async {
        final sourcePath = '${rootFor(key)}/config/source';
        await remember(sourcePath);
        await command(Cmds.patchSet(ch, 'In', 1));
        await native.agrees(sourcePath, 1);
        Object headamp;
        try {
          headamp = await native.read(
            '/-ha/${ch.toString().padLeft(2, '0')}/index',
          );
        } catch (e) {
          throw Blocked(
            'Emulator cannot independently resolve physical headamp: $e',
          );
        }
        if (headamp is! int || headamp < 0)
          throw Blocked('Source has no physical headamp');
        socket.add(Cmds.focus([ch]));
        await until(
          () =>
              values['$ch/x32:preamp_state'] is String &&
              (jsonDecode(values['$ch/x32:preamp_state'] as String)
                      as Map)['headamp'] ==
                  headamp &&
              values['$ch/source'] == 'In01',
          'Fresh source-aware preamp descriptor',
        );
        final context = <String, Object>{
          'source': values['$ch/source']!,
          'x32:preamp_state': values['$ch/x32:preamp_state']!,
        };
        final root = '/headamp/${headamp.toString().padLeft(3, '0')}';
        final gain = await remember('$root/gain');
        final desiredGain = gain == 0 ? 60.0 : -12.0;
        await command(Cmds.paramGuarded(ch, 'gain', desiredGain, context));
        await native.agrees('$root/gain', desiredGain == 60 ? 1.0 : 0.0);
        final phantom = await remember('$root/phantom');
        final desiredPhantom = phantom == 0;
        await command(
          Cmds.paramGuarded(ch, 'phantom', desiredPhantom, context),
        );
        await native.agrees('$root/phantom', desiredPhantom ? 1 : 0);
        final stale = <String, Object>{...context, 'source': 'OFF'};
        await command(
          Cmds.paramGuarded(ch, 'phantom', !desiredPhantom, stale),
          reject: true,
        );
        await native.agrees('$root/phantom', desiredPhantom ? 1 : 0);
      });
    }
    for (final field in [
      (id: 'eq1_gain', path: '/ch/01/eq/1/g', wire: 0.5, value: 0.0),
      (id: 'comp_thr', path: '/ch/01/dyn/thr', wire: 0.5, value: -30.0),
      (id: 'send1_on', path: '/ch/01/mix/01/on', wire: 1, value: true),
      (id: 'dca1_assign', path: '/ch/01/grp/dca', wire: 1, value: true),
    ]) {
      await check(
        'External ${field.id} promptly reaches observed engine state',
        () async {
          await remember(field.path);
          // First force the opposite endpoint so a cached equal value cannot pass.
          final opposite = field.wire == 1 ? 0 : 0.0;
          await native.write(field.path, opposite);
          await native.agrees(field.path, opposite);
          final start = sequence;
          await native.write(field.path, field.wire);
          await until(
            () =>
                (valueSequence['0/${field.id}'] ?? 0) > start &&
                same(values['0/${field.id}'], field.value),
            'External ${field.id} observation',
          );
        },
      );
    }
  }
}

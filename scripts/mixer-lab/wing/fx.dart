import 'dart:convert';
import 'dart:io';

import '../../../app/lib/engine_protocol.dart';
import '../osc.dart';
import 'client.dart';

extension WingFx on WingProbe {
  Future<void> fxPass() async {
    // Frozen upstream WING 3.0.5 protocol facts; no product codec is imported.
    final reference = jsonDecode(
      await File.fromUri(Platform.script.resolve('wing/fx-reference.json'))
          .readAsString(),
    ) as Map;
    final models = (reference['models'] as Map).cast<String, dynamic>();
    final advertised = (caps!['fx']['types'] as List).cast<Map>();
    await check('FX advertised models match independent WING schema', () async {
      final ids = advertised.map((m) => m['id']).toSet();
      require(
        ids.length == models.length && ids.containsAll(models.keys),
        'FX model catalog differs from reference',
      );
      for (final row in advertised) {
        final expected = models[row['id']] as Map;
        require(
          (row['params'] as List).length == (expected['params'] as List).length,
          'FX ${row['id']} parameter count differs',
        );
      }
    });
    for (var slot = 1; slot <= 16; slot++) {
      final root = '/fx/$slot';
      Object original;
      try {
        original = await remember('$root/mdl');
        if (original != 'NONE')
          throw Blocked(
            'Original model $original has uncaptured model parameters',
          );
        await remember('$root/fxmix');
      } catch (e) {
        results.add({
          'check': 'FX slot $slot fixture capture',
          'status': 'blocked',
          'reason': '$e',
        });
        continue;
      }
      for (final entry in models.entries) {
        final id = entry.key, model = entry.value as Map;
        if (slot > 8 && model['standard'] != true) continue;
        final params = (model['params'] as List).cast<Map>();
        var selected = false;
        await check(
          'FX slot $slot $id model selection',
          () async {
            await command(Cmds.fxSetType(slot, id));
            await native.agrees('$root/mdl', id);
            selected = true;
          },
          evidence: 'protocol_readback_model_binding_not_individually_audited',
        );
        final indices = slot == 1 || slot == 9
            ? List<int>.generate(params.length, (i) => i)
            : <int>[
                if (params.isNotEmpty) 0,
                if (params.length > 1) params.length - 1,
              ];
        for (final i in indices) {
          final p = params[i], name = p['name'] as String;
          await check(
            'FX slot $slot $id par${i + 1} $name native endpoint',
            () async {
              if (!selected) throw Blocked('Model selection failed');
              final path = '$root/$name';
              Object before;
              try {
                before = p['type'] == 'integer'
                    ? await readInteger(
                        path,
                        (p['min'] as num).toInt(),
                        (p['max'] as num).toInt(),
                      )
                    : await native.read(path);
              } catch (e) {
                throw Blocked('Native model parameter unavailable: $path ($e)');
              }
              final choices = (p['choices'] as List);
              final kind = p['type'] as String;
              final Object low = kind == 'string enum' || kind == 'float enum'
                  ? choices.first
                  : p['min'];
              final Object high = kind == 'string enum' || kind == 'float enum'
                  ? choices.last
                  : p['max'];
              if (same(low, high)) throw Blocked('Constant parameter');
              final desired = same(before, high) ? low : high;
              final normalized = same(desired, high) ? 1.0 : 0.0;
              await command(
                Cmds.fxParam(
                  slot,
                  'par${(i + 1).toString().padLeft(2, '0')}',
                  normalized,
                ),
              );
              if (kind == 'integer') {
                await agreesInteger(
                  path,
                  (desired as num).toInt(),
                  (p['min'] as num).toInt(),
                  (p['max'] as num).toInt(),
                );
              } else {
                await native.agrees(path, desired);
              }
            },
            evidence:
                'protocol_readback_model_binding_not_individually_audited',
          );
        }
      }
      if (slot > 8) {
        await check('FX slot $slot rejects premium model', () async {
          await command(Cmds.fxSetType(slot, 'HALL'), reject: true);
        });
      }
    }
    await check('Unavailable FX bypass and on reject', () async {
      await command(Cmds.fxBypass(1, true), reject: true);
      await command(Cmds.fxOn(1, false), reject: true);
    });
  }
}

// Manual output-strip DSP guard coverage on the isolated X32 fixture.
import 'dart:convert';
import 'dart:io';

import '../../../app/lib/engine_protocol.dart';
import '../osc.dart';
import '../verify-input-processors.dart';

const _outputs = [
  (key: 'bus/01', channel: 48, root: '/bus/01', choices: 6, partner: 49),
  (key: 'mtx/01', channel: 64, root: '/mtx/01', choices: 14, partner: 65),
  (key: 'main', channel: 70, root: '/main/st', choices: 14, partner: null),
];

extension X32OutputProcessors on ProcessorProbe {
  Future<void> outputProcessors(Uri uri) async {
    for (final fixture in _outputs) {
      await check(
        '${fixture.key} six-band EQ and compressor native capture',
        () async {
          final flags = <String, Object>{};
          for (final path in [
            '${fixture.root}/eq/on',
            '${fixture.root}/dyn/on',
          ]) {
            flags[path] = await remember(path);
          }
          for (final processor in ['eq', 'compressor']) {
            final snapshot = await capture(
              processor,
              channel: fixture.channel,
              key: fixture.key,
            );
            final links = snapshot['links'] as List;
            require(
              fixture.partner == null
                  ? links.isEmpty && snapshot['link_mode'] == 'non_linkable'
                  : links.length == 1 &&
                        links.single['value'] == false &&
                        snapshot['link_mode'] == 'unlinked',
              'Incorrect output link witness: $links',
            );
            if (processor == 'eq') {
              for (var band = 1; band <= 6; band++) {
                for (final field in ['freq', 'gain', 'q', 'type']) {
                  require(
                    (snapshot['rows'] as List).whereType<Map>().any(
                      (r) => r['id'] == 'eq${band}_$field',
                    ),
                    'Output EQ snapshot omitted band $band $field',
                  );
                  final r = row(snapshot, 'eq${band}_$field');
                  final expected =
                      '${fixture.root}/eq/$band/${field == 'gain'
                          ? 'g'
                          : field == 'freq'
                          ? 'f'
                          : field}';
                  require(
                    r['native'] == expected && r['revision'] is int,
                    'Incorrect band mapping for ${fixture.key}: $r',
                  );
                  final nativeValue = await native.read(expected);
                  if (field == 'type') {
                    require(
                      same(nativeValue, r['value']),
                      'EQ type readback mismatch at $expected',
                    );
                  }
                }
              }
            } else {
              require(
                (snapshot['rows'] as List).whereType<Map>().any(
                  (r) => r['id'] == 'comp_mode',
                ),
                'Output compressor omitted mode',
              );
              final mode = row(snapshot, 'comp_mode');
              final value = await native.read('${fixture.root}/dyn/mode');
              require(
                mode['native'] == '${fixture.root}/dyn/mode' &&
                    same(value, mode['value']),
                'Compressor mode mapping/readback mismatch: $mode',
              );
            }
          }
          for (final entry in flags.entries) {
            await native.agrees(entry.key, entry.value);
          }
        },
      );
      await check('${fixture.key} output EQ choice domain and fixture limits', () async {
        final params = (caps!['params'] as List).whereType<Map>();
        for (var band = 1; band <= 6; band++) {
          final types = params.where((p) => p['id'] == 'eq${band}_type').single;
          final presentations = (types['presentations'] as List)
              .whereType<Map>();
          final choice = presentations
              .where(
                (p) =>
                    p['when'] is Map &&
                    (p['when'] as Map)['id'] == 'x32:eq_type_count' &&
                    (p['when'] as Map)['value'] == fixture.choices,
              )
              .single;
          final choices = (choice['choices'] as List).whereType<Map>().toList();
          require(
            choices.length == fixture.choices &&
                List.generate(fixture.choices, (i) => i).every(
                  (i) => choices.any(
                    (c) => c['value'] == i && c['label'] is String,
                  ),
                ),
            'Output band $band shape catalogue does not match ${fixture.choices} native options',
          );
        }
        if (fixture.choices == 6) {
          final path = '${fixture.root}/eq/1/type';
          final before = await remember(path);
          final savedBand = <String, Object>{path: before};
          for (final suffix in ['f', 'g', 'q']) {
            final leaf = '${fixture.root}/eq/1/$suffix';
            savedBand[leaf] = await remember(leaf);
          }
          try {
            final snapshot = await capture(
              'eq',
              channel: fixture.channel,
              key: fixture.key,
            );
            await reject(
              'eq',
              snapshot,
              'eq1_type',
              6,
              path,
              before,
              channel: fixture.channel,
              key: fixture.key,
            );
            final desired = before == 5 ? 0 : 5;
            final result = await request(
              'eq',
              channel: fixture.channel,
              key: fixture.key,
              field: 'eq1_type',
              value: desired,
              guard: snapshot,
            );
            require(
              result['message'] == null && result['snapshot'] is Map,
              'Bus EQ shape write unconfirmed: $result',
            );
            await native.agrees(path, desired);
          } finally {
            for (final leaf in savedBand.entries) {
              await native.write(leaf.key, leaf.value);
              await native.agrees(leaf.key, leaf.value);
            }
          }
        }
        if (fixture.choices == 14) {
          checks.add({
            'check': '${fixture.key} EQ shapes 6–13 native writes',
            'status': 'blocked',
            'reason': 'Pinned Sidecar implements only six EQ shapes; no success is inferred for the remaining eight native options.',
          });
        }
      });
      await check(
        '${fixture.key} EQ guarded write, stale sibling and session',
        () async {
          final target = '${fixture.root}/eq/1/g';
          final sibling = '${fixture.root}/eq/2/g';
          final original = await remember(target);
          final other = await remember(sibling);
          final flags = {
            for (final path in [
              '${fixture.root}/eq/on',
              '${fixture.root}/dyn/on',
            ])
              path: await native.read(path),
          };
          final snapshot = await capture(
            'eq',
            channel: fixture.channel,
            key: fixture.key,
          );
          final nativeWire = same(original, 0.75) ? 0.5 : 0.75;
          final engineering = nativeWire == 0.75 ? 7.5 : 0.0;
          await reject(
            'eq',
            snapshot,
            'eq1_gain',
            engineering,
            target,
            original,
            channel: fixture.channel,
            key: fixture.key,
            overrideEpoch: epoch! + 1,
          );
          final siblingWire = same(other, 0.0) ? 1.0 : 0.0;
          await native.write(sibling, siblingWire);
          await native.agrees(sibling, siblingWire);
          await reject(
            'eq',
            snapshot,
            'eq1_gain',
            engineering,
            target,
            original,
            channel: fixture.channel,
            key: fixture.key,
          );
          await native.write(sibling, other);
          await native.agrees(sibling, other);
          final fresh = await capture(
            'eq',
            channel: fixture.channel,
            key: fixture.key,
          );
          final result = await request(
            'eq',
            channel: fixture.channel,
            key: fixture.key,
            field: 'eq1_gain',
            value: engineering,
            guard: fresh,
          );
          require(
            result['message'] == null && result['snapshot'] is Map,
            'Output EQ write unconfirmed: $result',
          );
          await native.agrees(target, nativeWire);
          require(
            same(
              row(result['snapshot'] as Map, 'eq1_gain')['value'],
              engineering,
            ),
            'Output EQ engineering readback mismatch',
          );
          for (final flag in flags.entries) {
            await native.agrees(flag.key, flag.value);
          }
        },
      );
      await check('${fixture.key} compressor mode guarded write', () async {
        final target = '${fixture.root}/dyn/mode';
        final original = await remember(target);
        require(
          original == 0 || original == 1,
          'Invalid native compressor mode',
        );
        final flags = {
          for (final path in [
            '${fixture.root}/eq/on',
            '${fixture.root}/dyn/on',
          ])
            path: await native.read(path),
        };
        final guard = await capture(
          'compressor',
          channel: fixture.channel,
          key: fixture.key,
        );
        final saved = <String, Object>{target: original};
        for (final r in (guard['rows'] as List).whereType<Map>()) {
          final nativePath = r['native'] as String;
          saved[nativePath] = await remember(nativePath);
        }
        try {
          final desired = original == 0 ? 1 : 0;
          final result = await request(
            'compressor',
            channel: fixture.channel,
            key: fixture.key,
            field: 'comp_mode',
            value: desired,
            guard: guard,
          );
          require(
            result['message'] == null && result['snapshot'] is Map,
            'Output compressor write unconfirmed: $result',
          );
          await native.agrees(target, desired);
          require(
            same(row(result['snapshot'] as Map, 'comp_mode')['value'], desired),
            'Compressor mode snapshot mismatch',
          );
          for (final flag in flags.entries) {
            await native.agrees(flag.key, flag.value);
          }
        } finally {
          for (final leaf in saved.entries) {
            await native.write(leaf.key, leaf.value);
            await native.agrees(leaf.key, leaf.value);
          }
        }
      });
      await check(
        '${fixture.key} compressor threshold engineering readback',
        () async {
          final path = '${fixture.root}/dyn/thr';
          final original = await remember(path);
          // X32 protocol: compressor threshold is linear -60..0 dB.
          final wire = same(original, 0.75) ? 0.5 : 0.75;
          final db = wire == 0.75 ? -15.0 : -30.0;
          final flags = {
            for (final leaf in [
              '${fixture.root}/eq/on',
              '${fixture.root}/dyn/on',
            ])
              leaf: await native.read(leaf),
          };
          final guard = await capture(
            'compressor',
            channel: fixture.channel,
            key: fixture.key,
          );
          final result = await request(
            'compressor',
            channel: fixture.channel,
            key: fixture.key,
            field: 'comp_thr',
            value: db,
            guard: guard,
          );
          require(
            result['message'] == null && result['snapshot'] is Map,
            'Output compressor threshold write unconfirmed: $result',
          );
          await native.agrees(path, wire);
          require(
            same(row(result['snapshot'] as Map, 'comp_thr')['value'], db),
            'Output compressor threshold engineering readback mismatch',
          );
          for (final flag in flags.entries) {
            await native.agrees(flag.key, flag.value);
          }
        },
      );
      if (fixture.partner != null) {
        await check(
          '${fixture.key} stereo partner lock rejects output write',
          () async {
            final path = '${fixture.root}/eq/1/g';
            final before = await native.read(path);
            final guard = await capture(
              'eq',
              channel: fixture.channel,
              key: fixture.key,
            );
            final other = await WebSocket.connect(uri.toString());
            final events = <Map<String, dynamic>>[];
            final subscription = other.listen((raw) {
              if (raw is String) {
                final e = jsonDecode(raw);
                if (e is Map<String, dynamic>) events.add(e);
              }
            });
            var locked = false;
            try {
              other.add(Cmds.hello(false, true));
              await until(
                () => events.any((e) => e['type'] == 'hello'),
                'Second client hello',
              );
              other.add(jsonEncode({'cmd': 'lock', 'ch': fixture.partner}));
              await until(
                () => locks.containsKey('${fixture.partner}'),
                'Output partner lock',
              );
              locked = true;
              await reject(
                'eq',
                guard,
                'eq1_gain',
                0.0,
                path,
                before,
                channel: fixture.channel,
                key: fixture.key,
              );
            } finally {
              if (locked) {
                other.add(jsonEncode({'cmd': 'unlock', 'ch': fixture.partner}));
                await until(
                  () => !locks.containsKey('${fixture.partner}'),
                  'Output partner unlock',
                );
              }
              await subscription.cancel();
              await other.close();
            }
          },
        );
      }
    }
  }
}

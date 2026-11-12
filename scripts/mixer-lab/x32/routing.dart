import 'dart:async';
import 'dart:convert';
import 'dart:math' as math;

import '../../../app/lib/engine_protocol.dart';
import '../../../app/lib/core/destination_controls.dart';
import '../osc.dart';
import 'client.dart';
import 'fixtures.dart';

extension X32Routing on X32Probe {
  Future<void> routingPass({bool focused = false, int cycle = 0}) async {
    // Every advertised routing destination is inventoried, including emulator holes.
    final routing = caps!['routing'] as Map;
    for (final plane in routing['planes'] as List) {
      for (final d in plane['destinations'] as List) {
        final id = d['id'] as String;
        if (focused && !id.startsWith('fx/source/')) continue;
        await check(
          'Routing $id guarded patch and native readback${focused ? ' cycle $cycle' : ''}',
          () async {
            final path = routePath(id);
            if (path == null)
              throw Blocked(
                'No independent fixture path for routing destination',
              );
            Object original;
            try {
              original = await remember(path);
            } catch (e) {
              throw Blocked(
                'Independent emulator routing leaf unavailable: $path ($e)',
              );
            }
            final routes = (latest['routing']?['destinations'] as List? ?? [])
                .cast<Map>();
            final found = routes.where((r) => r['destination'] == id).toList();
            if (found.isEmpty ||
                found.first['guard'] == null && d['output'] == true) {
              throw Blocked(
                'No independently observed provider routing guard/label available',
              );
            }
            final current = found.first;
            final groups = (d['source_groups'] as List).cast<Map>();
            var offset = id.startsWith('input/') ? 1 : 0;
            Map? source;
            int? wire;
            for (final g in groups) {
              final count = g['count'] as int;
              for (var n = 1; n <= math.min(count, 2); n++) {
                if (offset + n - 1 != original) {
                  source = {'group': g['key'], 'index': n};
                  wire = offset + n - 1;
                  break;
                }
              }
              if (source != null) break;
              offset += count;
            }
            require(source != null, 'No distinct native routing source');
            await command(
              jsonEncode({
                'cmd': 'route_patch',
                'driver': 'x32',
                'epoch': routing['epoch'],
                'destination': id,
                'expected': current['label'],
                'guard': current['guard'],
                'source': source,
              }),
            );
            await native.agrees(path, wire!);
          },
        );
      }
    }
    final handledControls = <String>{};
    final controlRows =
        (latest['destination_controls']?['controls'] as List? ?? [])
            .cast<Map>();
    for (final descriptor in controlRows) {
      final target = descriptor['target'] as String;
      final id = descriptor['id'] as String;
      if (!target.startsWith('outputs/')) continue;
      if (!['tap', 'invert', 'delay_on', 'delay_time'].contains(id)) continue;
      handledControls.add('$target/$id');
      await check(
        'Output processor $target/$id guarded write${focused ? ' cycle $cycle' : ''}',
        () async {
          final epoch = latest['destination_controls']!['epoch'] as int;
          List<DestinationControlVm> block() => [
            for (final r in latest['destination_controls']!['controls'] as List)
              if (r['target'] == target)
                if (DestinationControlVm.fromJson(epoch, r) case final c?) c,
          ];
          try {
            await until(
              () => block().any((r) => r.id == id && r.value != null),
              'Fresh provider output descriptor $target/$id',
            );
          } on TimeoutException {
            throw Blocked(
              'Provider withdrew $target/$id and did not publish a fresh observed descriptor within the normal deadline; no guarded write sent',
            );
          }
          final rows = block();
          final row = rows.singleWhere((r) => r.id == id);
          final suffix = {
            'tap': 'pos',
            'invert': 'invert',
            'delay_on': 'delay/on',
            'delay_time': 'delay/time',
          }[id]!;
          final path = '/$target/$suffix';
          await remember(path);
          final desired = id == 'delay_time'
              ? (row.value! < 250 ? 500.0 : 0.3)
              : (row.value == 0 ? 1.0 : 0.0);
          await command(
            Cmds.destinationParam(
              'full-$target-$id-cycle$cycle',
              row,
              desired,
              guard: rows,
            ),
          );
          await native.agrees(
            path,
            id == 'delay_time' ? (desired == 500 ? 1.0 : 0.0) : desired.toInt(),
          );
        },
      );
    }
    results.add({
      'check': 'Output control inventory${focused ? ' cycle $cycle' : ''}',
      'status': 'inventory',
      'exercised': handledControls.toList(),
    });
  }

  Future<void> routingStress() async {
    // hydration rather than recording a cold, empty roster as coverage.
    await until(() {
      final routes = (latest['routing']?['destinations'] as List? ?? [])
          .whereType<Map>();
      final fx = routes.where(
        (r) =>
            '${r['destination']}'.startsWith('fx/source/') &&
            r['guard'] != null,
      );
      final frame = latest['destination_controls'];
      if (fx.length != 8 || frame == null) return false;
      final rows = <DestinationControlVm>[
        for (final raw in frame['controls'] as List)
          if (DestinationControlVm.fromJson(frame['epoch'] as int, raw)
              case final row?)
            row,
      ];
      return ['tap', 'invert', 'delay_on', 'delay_time'].every(
            (id) => rows.any(
              (r) =>
                  r.target == 'outputs/main/01' &&
                  r.id == id &&
                  r.value != null,
            ),
          ) &&
          rows.any(
            (r) =>
                r.target == 'outputs/main/02' &&
                r.id == 'invert' &&
                r.value != null,
          );
    }, 'Independently observed routing/output control hydration');
    results.add({
      'check': 'Observed routing/output control hydration',
      'status': 'pass',
    });
    for (var cycle = 1; cycle <= 20; cycle++) {
      await routingPass(focused: true, cycle: cycle);
    }
    require(
      results
              .where(
                (r) =>
                    r['status'] == 'pass' &&
                    '${r['check']}'.startsWith('Routing fx/source/'),
              )
              .length ==
          160,
      'Every FX source destination must pass in every stress cycle',
    );
    require(
      results
              .where(
                (r) =>
                    r['status'] == 'pass' &&
                    '${r['check']}'.startsWith(
                      'Output processor outputs/main/02/invert',
                    ),
              )
              .length ==
          20,
      'Critical output invert must pass in every stress cycle',
    );
  }
}

import 'dart:async';
import 'dart:convert';
import 'dart:math' as math;

import '../../../app/lib/engine_protocol.dart';
import '../../../app/lib/core/destination_controls.dart';
import '../osc.dart';
import 'client.dart';
import 'fixtures.dart';

extension X32Routing on X32Probe {
  Future<void> _fxGuardedPatch({
    required String id,
    required String path,
    required int epoch,
    required Object original,
    required Map current,
    required Map source,
  }) async {
    var descriptor = current;
    for (var attempt = 1; attempt <= 3; attempt++) {
      require(latest['sync_epoch']?['epoch'] == epoch, 'Routing epoch changed');
      final nativeValue = await native.read(path);
      require(
        same(nativeValue, original),
        'Native source changed before guarded write at $id',
      );
      require(
        nativeValue is int &&
            descriptor['label'] == 'Source ${nativeValue + 1}',
        'Routing guard does not match independent native source at $id',
      );
      final frame = routingSequence;
      final guard = descriptor['guard'];
      require(guard != null, 'Routing guard unavailable at $id');
      final start = errors.length;
      socket.add(
        jsonEncode({
          'cmd': 'route_patch',
          'driver': 'x32',
          'epoch': epoch,
          'destination': id,
          'expected': descriptor['label'],
          'guard': guard,
          'source': source,
        }),
      );
      await barrier();
      final found = errors.sublist(start);
      results.add({
        'check': 'FX routing guard attempt $id',
        'status': 'evidence',
        'attempt': attempt,
        'frame': frame,
        'epoch': epoch,
        'native_before': nativeValue,
        'guard': guard,
        'outcome': found.isEmpty ? 'accepted' : 'rejected',
        'errors': found,
      });
      if (found.isEmpty) return;
      require(
        found.length == 1 &&
            (found.single['message'] ==
                    'Output source/revision/session changed' ||
                found.single['message'] ==
                    'x32: waiting for independent route readback at $id'),
        'FX routing command failed outside the pre-dispatch guard: $found',
      );
      require(attempt < 3, 'FX routing guard changed on all three attempts');
      final after = await native.read(path);
      require(
        same(after, original),
        'Native source changed after guard rejection at $id',
      );
      Map? refreshed;
      await until(() {
        require(
          latest['sync_epoch']?['epoch'] == epoch,
          'Routing epoch changed',
        );
        if (routingSequence <= frame) return false;
        final rows = (latest['routing']?['destinations'] as List? ?? [])
            .whereType<Map>();
        refreshed = rows.where((row) => row['destination'] == id).firstOrNull;
        return refreshed?['guard'] != null &&
            jsonEncode(refreshed!['guard']) != jsonEncode(guard) &&
            refreshed?['label'] == 'Source ${(after as int) + 1}';
      }, 'Fresh routing guard after rejected FX request $id');
      descriptor = refreshed!;
    }
  }

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
            final paired = id.startsWith('fx/source/') && id.endsWith('/l')
                ? '${id.substring(0, id.length - 1)}r'
                : null;
            if (paired != null) await remember(routePath(paired)!);
            final oldPair = paired == null
                ? null
                : routes.where((r) => r['destination'] == paired).firstOrNull;
            if (paired != null && oldPair?['guard'] == null) {
              throw Blocked('No observed FX companion guard for $paired');
            }
            final oldPairGuard = oldPair?['guard'];
            final beforeFrame = routingSequence;
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
            if (id.startsWith('fx/source/')) {
              await _fxGuardedPatch(
                id: id,
                path: path,
                epoch: routing['epoch'] as int,
                original: original,
                current: current,
                source: source!,
              );
            } else {
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
            }
            await native.agrees(path, wire!);
            if (paired != null) {
              final pairedPath = routePath(paired)!;
              final pairValue = await native.read(pairedPath);
              require(pairValue is int, 'Invalid native FX companion source');
              Map? afterLeft;
              Map? afterPair;
              await until(() {
                if (routingSequence <= beforeFrame ||
                    latest['sync_epoch']?['epoch'] != routing['epoch']) {
                  return false;
                }
                final rows = (latest['routing']?['destinations'] as List? ?? [])
                    .whereType<Map>();
                afterLeft = rows
                    .where((r) => r['destination'] == id)
                    .firstOrNull;
                afterPair = rows
                    .where((r) => r['destination'] == paired)
                    .firstOrNull;
                return afterLeft?['guard'] != null &&
                    afterPair?['guard'] != null &&
                    jsonEncode(afterPair!['guard']) !=
                        jsonEncode(oldPairGuard) &&
                    afterLeft?['label'] == 'Source ${wire! + 1}' &&
                    afterPair?['label'] == 'Source ${(pairValue as int) + 1}';
              }, 'Fresh independently observed FX pair $id / $paired');
              results.add({
                'check': 'FX pair guard refresh $id / $paired',
                'status': 'evidence',
                'before_frame': beforeFrame,
                'after_frame': routingSequence,
                'epoch': routing['epoch'],
                'old_companion_guard': oldPairGuard,
                'new_companion_guard': afterPair!['guard'],
                'left_guard': afterLeft!['guard'],
                'native_left': wire,
                'native_companion': pairValue,
              });
            }
          },
        );
      }
    }
    final handledControls = <String>{};
    final controlRows =
        (latest['destination_controls']?['controls'] as List? ?? [])
            .cast<Map>();
    await check('Output control descriptor inventory is present', () async {
      require(
        controlRows.any((r) => '${r['target']}'.startsWith('outputs/')),
        'No output controls observed; an empty sweep cannot establish coverage',
      );
      for (final id in ['tap', 'invert', 'delay_on', 'delay_time']) {
        require(
          controlRows.any(
            (r) => '${r['target']}'.startsWith('outputs/') && r['id'] == id,
          ),
          'No advertised output $id control was observed',
        );
      }
    });
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

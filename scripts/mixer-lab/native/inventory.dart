import '../osc.dart';
import 'client.dart';
import '../../../app/lib/core/native_library.dart';

extension NativeInventoryChecks on NativeProbe {
  Future<void> verifyInventory() async {
    await check(
      'Native inspect parses through Flutter descriptor contract',
      () async {
        final current = await inspect();
        initialInventories.add(current.guard);
        require(
          current.supports('inspect') && current.supports('refresh'),
          'Missing read-only native entry points',
        );
      },
    );
    await check('Independent native inventory prerequisites', () async {
      final paths = [
        '/ch/32/mix/fader',
        '/-show/showfile/scene/000/hasdata',
        '/-show/showfile/snippet/000/hasdata',
        '/-show/showfile/cue/000/numb',
        '/-libs/ch/001/hasdata',
      ];
      final results = await Future.wait(
        paths.map((path) async {
          final watch = Stopwatch()..start();
          try {
            final value = await osc.read(path);
            return <String, Object?>{
              'path': path,
              'status': 'answered',
              'value': value,
              'elapsed_ms': watch.elapsedMilliseconds,
            };
          } catch (error) {
            return <String, Object?>{
              'path': path,
              'status': 'unavailable',
              'reason': '$error',
              'elapsed_ms': watch.elapsedMilliseconds,
            };
          }
        }),
      );
      prerequisiteEvidence.addAll(results);
      final controlAlive = results.first['status'] == 'answered';
      final showMissing = results
          .skip(1)
          .take(3)
          .every((row) => row['status'] == 'unavailable');
      if (controlAlive && showMissing) {
        nativeInventoryBlocked = 'Independent OSC control read succeeds but required scene/000/hasdata, snippet/000/hasdata and cue/000/numb do not answer; pinned emulator lacks native-show inventory protocol';
        throw Blocked(nativeInventoryBlocked!);
      }
      require(
        controlAlive,
        'Independent control liveness unavailable; cannot attribute native silence to emulator',
      );
    });
    await check('Native refresh fails closed promptly for independently missing inventory protocol', () async {
      if (nativeInventoryBlocked == null)
        throw Blocked(
          'Required inventory paths answered; missing-protocol case not applicable',
        );
      final current = await inspect();
      final watch = Stopwatch()..start();
      final result = await request({'op': 'refresh'}, expected: current.guard);
      final inventory = NativeLibraryVm.parse(result['inventory']);
      require(
        result['status'] == 'uncertain' &&
            inventory != null &&
            !inventory.complete &&
            '${result['message']}'.contains(
              'required native show inventory unavailable',
            ),
        'Missing inventory was not reported uncertain/incomplete: $result',
      );
      require(
        watch.elapsedMilliseconds < 5000,
        'Required native contract failure did not fail fast',
      );
    });
    await check(
      'Native show, scene, snippet, cue and library inventory refresh',
      () async {
        final current = await refresh();
        initialInventories.add(current.guard);
        require(
          current.guard['scenes'] is List &&
              current.guard['snippets'] is List &&
              current.guard['cues'] is List,
          'Native inventory missing banks',
        );
      },
    );
    await check('Stale native guard rejects before mutation', () async {
      final current = await inspect();
      final stale = Map<String, dynamic>.from(current.guard)..['revision'] = -1;
      final result = await request({'op': 'refresh'}, expected: stale);
      require(
        result['status'] == 'rejected',
        'Stale native guard was accepted',
      );
    });
  }
}

import '../osc.dart';
import 'client.dart';

extension NativeShowChecks on NativeProbe {
  Future<void> verifyShow() async {
    for (final kind in ['scene', 'snippet']) {
      final bank = '${kind}s';
      int? slot;
      await check('$kind empty owned-slot selection', () async {
        final current = await refresh();
        if (!current.supports('${kind}_save'))
          throw Blocked('$kind operations not advertised');
        final occupied = (current.guard[bank] as List)
            .cast<Map>()
            .map((r) => r['slot'])
            .toSet();
        for (var candidate = 99; candidate >= 0; candidate--) {
          if (occupied.contains(candidate)) continue;
          final base =
              '/-show/showfile/$kind/${candidate.toString().padLeft(3, '0')}';
          final name = await osc.read('$base/name'),
              data = await osc.read('$base/hasdata');
          if ((name == '' || name == '-') && data == 0) {
            slot = candidate;
            break;
          }
        }
        if (slot == null)
          throw Blocked(
            'No independently empty $kind slot; existing entries preserved',
          );
      });
      Future<int> selected() async {
        if (slot == null) throw Blocked('No empty $kind slot prerequisite');
        return slot!;
      }

      await check('$kind save independently observes native name', () async {
        final chosen = await selected();
        final payload = <String, dynamic>{
          'op': '${kind}_save',
          'slot': chosen,
          'name': '$runTag-$kind',
        };
        ownedSlots[kind] = chosen;
        await action(payload);
        await osc.agrees(
          '/-show/showfile/$kind/${chosen.toString().padLeft(3, '0')}/name',
          '$runTag-$kind',
        );
      });
      await check('$kind rename independently observes native name', () async {
        final chosen = await selected();
        await action({
          'op': '${kind}_rename',
          'slot': chosen,
          'name': '$runTag-renamed',
        });
        await osc.agrees(
          '/-show/showfile/$kind/${chosen.toString().padLeft(3, '0')}/name',
          '$runTag-renamed',
        );
      });
      await check(
        '$kind recall receives native correlated status (contents remain uncertain)',
        () async {
          final chosen = await selected();
          final result = await action({'op': '${kind}_recall', 'slot': chosen});
          require(
            '${result['message']}'.contains('accepted'),
            'No independent native recall acceptance: $result',
          );
        },
      );
      await check(
        '$kind delete clears owned-slot metadata independently',
        () async {
          final chosen = await selected();
          await action({'op': '${kind}_delete', 'slot': chosen});
          final name = await osc.read(
            '/-show/showfile/$kind/${chosen.toString().padLeft(3, '0')}/name',
          );
          require(
            name == '' || name == '-',
            'Deleted $kind name remains $name',
          );
          ownedSlots.remove(kind);
        },
      );
    }

    await check(
      'Native cue add observes an independently empty fixture position',
      () async {
        final current = await refresh();
        if ((current.guard['cues'] as List).isNotEmpty)
          throw Blocked('Existing cues preserved; use a fresh empty fixture');
        const base = '/-show/showfile/cue/000';
        for (final field in [
          'numb',
          'name',
          'skip',
          'scene',
          'snippet',
          'miditype',
          'midichan',
          'midipara1',
          'midipara2',
        ]) {
          originals['$base/$field'] = await osc.read('$base/$field');
        }
        require(originals['$base/numb'] == 0, 'Cue position is occupied');
        await action({'op': 'cue_add', 'number': 49999, 'name': '$runTag-cue'});
        final after = await refresh();
        require(
          (after.guard['cues'] as List).any(
            (row) => row['name'] == '$runTag-cue',
          ),
          'Added cue absent from refreshed inventory',
        );
        await osc.agrees('$base/name', '$runTag-cue');
      },
    );
  }

  Future<void> cleanupShow() async {
    for (final entry in ownedSlots.entries.toList()) {
      await check('Cleanup owned ${entry.key} slot ${entry.value}', () async {
        final name = await osc.read(
          '/-show/showfile/${entry.key}/${entry.value.toString().padLeft(3, '0')}/name',
        );
        if (name is! String || !name.startsWith(runTag))
          throw Blocked('Slot ownership changed; refusing cleanup');
        await action({'op': '${entry.key}_delete', 'slot': entry.value});
        final cleared = await osc.read(
          '/-show/showfile/${entry.key}/${entry.value.toString().padLeft(3, '0')}/name',
        );
        require(
          cleared == '' || cleared == '-',
          'Owned slot cleanup not observed',
        );
        ownedSlots.remove(entry.key);
      });
    }
  }
}

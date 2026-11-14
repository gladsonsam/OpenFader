import '../osc.dart';
import 'client.dart';

extension NativePresetChecks on NativeProbe {
  Future<void> verifyPresets() async {
    var copyAttempted = false;
    for (final bank in ['ch', 'fx', 'r', 'mon']) {
      int? slot;
      final leaf = (int value) =>
          '/-libs/$bank/${value.toString().padLeft(3, '0')}';
      for (final op in ['save', 'rename', 'recall', 'delete']) {
        await check('Native $bank preset $op surface', () async {
          final current = await refresh();
          if (!current.supports('lib_$op'))
            throw Blocked(
              current.banksNote ?? 'Native library bank unavailable',
            );
          if (op == 'save') {
            final occupied = ((current.guard['libs'] as Map)[bank] as List)
                .cast<Map>()
                .map((r) => r['slot'])
                .toSet();
            for (var candidate = 100; candidate > 0; candidate--) {
              if (occupied.contains(candidate)) continue;
              final name = await osc.read('${leaf(candidate)}/name');
              final data = await osc.read('${leaf(candidate)}/hasdata');
              if ((name == '' || name == '-') && data == 0) {
                slot = candidate;
                break;
              }
            }
            if (slot == null)
              throw Blocked('No independently empty $bank preset slot');
          }
          if (slot == null) throw Blocked('No owned $bank preset prerequisite');
          final payload = <String, dynamic>{
            'op': 'lib_$op',
            'bank': bank,
            'slot': slot,
            if (op == 'save' || op == 'rename')
              'name': '$runTag-$bank${op == 'rename' ? '-r' : ''}',
            if (op == 'save' && (bank == 'ch' || bank == 'fx')) 'from': 0,
            if (op == 'recall' && (bank == 'ch' || bank == 'fx')) 'target': 0,
          };
          if (op == 'save')
            ownedPresets['$bank/$slot'] = {'bank': bank, 'slot': slot};
          final result = await action(payload, reload: false);
          if (op == 'save' || op == 'rename') {
            await osc.agrees('${leaf(slot!)}/name', payload['name'] as String);
          } else if (op == 'delete') {
            final name = await osc.read('${leaf(slot!)}/name');
            require(
              name == '' || name == '-',
              'Deleted $bank preset still named $name',
            );
            ownedPresets.remove('$bank/$slot');
          } else {
            require(
              '${result['message']}'.contains('accepted'),
              'No correlated preset recall acceptance',
            );
            if (bank == 'ch') {
              copyAttempted = true;
              await check(
                'Native channel preset copy owned source and destination',
                () async {
                  final current = await refresh();
                  if (!current.supports('libchan_copy'))
                    throw Blocked('Native copy not advertised');
                  final form = current.copyForms.singleWhere(
                    (f) => f.op == 'libchan_copy',
                  );
                  final destination = slot! - 1;
                  if (destination < 1)
                    throw Blocked('No safe destination below owned source');
                  final target =
                      '/-libs/ch/${destination.toString().padLeft(3, '0')}';
                  final name = await osc.read('$target/name'),
                      data = await osc.read('$target/hasdata');
                  if (!((name == '' || name == '-') && data == 0))
                    throw Blocked('Copy destination is occupied');
                  ownedPresets['ch/$destination'] = {
                    'bank': 'ch',
                    'slot': destination,
                  };
                  await action({
                    'op': form.op,
                    'src': slot! - 1,
                    'dst': destination - 1,
                    if (form.fields.any((f) => f.key == 'mask')) 'mask': 255,
                  });
                  await osc.agrees(
                    '$target/name',
                    await osc.read('${leaf(slot!)}/name'),
                  );
                  await action({
                    'op': 'lib_delete',
                    'bank': 'ch',
                    'slot': destination,
                  });
                  final cleared = await osc.read('$target/name');
                  require(
                    cleared == '' || cleared == '-',
                    'Copied preset cleanup not observed',
                  );
                  ownedPresets.remove('ch/$destination');
                },
              );
            }
            throw Blocked(
              'Recall acceptance observed, but restored preset contents were not independently verified',
            );
          }
        });
      }
    }
    await check('Native channel-preset copy form and availability', () async {
      if (copyAttempted) return;
      final current = await refresh();
      if (!current.supports('libchan_copy'))
        throw Blocked(current.banksNote ?? 'Channel preset copy unavailable');
      require(
        current.copyForms.any((form) => form.op == 'libchan_copy'),
        'Advertised copy operation missing app form',
      );
      throw Blocked(
        'No owned source preset available for safe copy; existing presets preserved',
      );
    });
  }

  Future<void> cleanupPresets() async {
    for (final entry in ownedPresets.entries.toList()) {
      await check('Cleanup owned preset ${entry.key}', () async {
        final bank = entry.value['bank'] as String,
            slot = entry.value['slot'] as int;
        final leaf = '/-libs/$bank/${slot.toString().padLeft(3, '0')}/name';
        final name = await osc.read(leaf);
        if (name is! String || !name.startsWith(runTag))
          throw Blocked('Preset ownership changed; refusing cleanup');
        await action({'op': 'lib_delete', 'bank': bank, 'slot': slot});
        final cleared = await osc.read(leaf);
        require(
          cleared == '' || cleared == '-',
          'Owned preset cleanup not observed',
        );
        ownedPresets.remove(entry.key);
      });
    }
  }
}

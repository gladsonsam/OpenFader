import '../../../app/lib/engine_protocol.dart';
import '../osc.dart';
import 'client.dart';
import 'fixtures.dart';

extension X32Fx on X32Probe {
  Future<void> fxPass() async {
    final fxTypes = (caps!['fx']['types'] as List).cast<Map>();
    // Snapshot original parameter leaves before selecting another algorithm.
    for (var slot = 1; slot <= 8; slot++) {
      await remember('/fx/$slot/type');
      final old = (await native.read('/fx/$slot/type') as num).toInt();
      final code = slot <= 4 ? fxTypes[old]['id'] as String : fx58[old];
      final oldDescriptor = fxTypes.singleWhere((r) => r['id'] == code);
      for (final p in oldDescriptor['params'] as List) {
        try {
          await remember('/fx/$slot/par/${(p['id'] as String).substring(3)}');
        } catch (_) {
          /* An absent old parameter has no state to restore. */
        }
      }
    }
    for (var slot = 1; slot <= 8; slot++) {
      final domain = slot <= 4
          ? fxTypes.map((r) => r['id'] as String).toList()
          : fx58;
      for (var nativeType = 0; nativeType < domain.length; nativeType++) {
        final code = domain[nativeType];
        final descriptor = fxTypes.singleWhere((r) => r['id'] == code);
        var typeReady = false;
        await check(
          'FX slot $slot $code type domain index $nativeType',
          () async {
            await command(Cmds.fxSetType(slot, code));
            await native.agrees('/fx/$slot/type', nativeType);
            typeReady = true;
          },
        );
        final parameters = (descriptor['params'] as List).cast<Map>();
        final selected = slot == 1
            ? parameters
            : [
                if (parameters.isNotEmpty) parameters.first,
                if (parameters.length > 1) parameters.last,
              ];
        if (!typeReady) {
          for (final p in selected)
            results.add({
              'check': 'FX $slot/$code/${p['id']}',
              'status': 'blocked',
              'reason': 'Algorithm selection failed; no parameter write sent',
            });
          continue;
        }
        for (final p in selected) {
          final id = p['id'] as String;
          final parameterNumber = int.parse(id.substring(3));
          if (code == 'FAC1M' && parameterNumber >= 8) {
            require(
              parameters.length == 14,
              'Hardware FAC1M catalogue must retain14 parameters',
            );
            results.add({
              'check': 'FX $slot/$code/$id native parameter type and value',
              'status': 'blocked',
              'reason': 'Primary author X32Fx.h documents14 FAC1M parameters with nativepar8 integer; pinned Sidecar1330613 livedaw-dsp/src/x32/fx_models.rs FAC1M has13 parameters and shifted semantics8..13. No mismatched algorithm write sent; native catalogue retained. Source: https://github.com/pmaillot/X32-Behringer/blob/master/X32Fx.h',
            });
            continue;
          }
          if (code == 'PIT2' && parameterNumber >= 12) {
            require(
              parameters.length == 13,
              'Hardware PIT2 catalogue must retain13 parameters',
            );
            results.add({
              'check': 'FX $slot/$code/$id native parameter type and value',
              'status': 'blocked',
              'reason': 'Latest primary protocol4.09-02 p118 documents PIT2par12Mix2 andpar13HiCut; pinned Sidecar1330613 livedaw-dsp/src/x32/fx_models.rs PIT2 has12 parameters withpar12HiCut. Controls12..13 have mismatched semantics, so no mismatched write sent; native13-parameter catalogue retained.',
            });
            continue;
          }
          if (code == 'PIT' && (parameterNumber == 4 || parameterNumber == 5)) {
            results.add({
              'check': 'FX $slot/$code/$id native parameter type and value',
              'status': 'blocked',
              'reason': 'Latest primary protocol4.09-02 p118 documents PITpar4Gain andpar5Pan; pinned Sidecar1330613 livedaw-dsp/src/x32/fx_models.rs PIT instead haspar4LoCut andpar5HiCut. A float echo would test a different control, so no mismatched write sent; native catalogue retained.',
            });
            continue;
          }
          await check('FX $slot/$code/$id native parameter type and value', () async {
            final path = '/fx/$slot/par/${id.substring(3)}';
            Object baseline;
            try {
              baseline = await native.read(path);
            } catch (e) {
              throw Blocked(
                'Emulator does not expose the selected algorithm parameter: $e',
              );
            }
            final choices = p['choices'] as List?;
            final enumValue = choices != null && choices.length > 1;
            final desired = baseline == 0 ? 1.0 : 0.0;
            await command(Cmds.fxParam(slot, id, desired));
            await native.agrees(
              path,
              enumValue ? (desired == 0 ? 0 : choices.length - 1) : desired,
            );
          });
        }
        if (slot != 1 && parameters.length > 2)
          results.add({
            'check': 'FX $slot/$code middle parameters',
            'status': 'not_exercised_here',
            'reason': 'Slot1 inventory carries algorithm schema coverage and corresponding emulator blockers',
            'not_exercised_here': parameters
                .sublist(1, parameters.length - 1)
                .map((p) => p['id'])
                .toList(),
          });
      }
    }
    await check(
      'Unavailable per-slot FX on/bypass writes reject explicitly',
      () async {
        await command(Cmds.fxOn(1, false), reject: true);
        await command(Cmds.fxBypass(1, true), reject: true);
      },
    );
  }
}

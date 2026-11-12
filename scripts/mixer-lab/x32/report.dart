import 'dart:convert';
import 'dart:io';

import 'client.dart';

extension X32Report on X32Probe {
  Future<void> finish(String reportPath, String? scenario) async {
    final restorationFailures = <String>[];
    for (final entry in originals.entries) {
      try {
        await native.write(entry.key, entry.value);
        await native.agrees(entry.key, entry.value);
      } catch (e) {
        restorationFailures.add('${entry.key}: $e');
      }
    }
    results.add({
      'check': 'Restore captured original native emulator state',
      'status': restorationFailures.isEmpty ? 'pass' : 'fail',
      'leaf_count': originals.length,
      'failures': restorationFailures,
    });
    await subscription.cancel();
    await socket.close();
    final report = {
      'time_utc': DateTime.now().toUtc().toIso8601String(),
      'fixture': 'x32',
      'scenario': scenario ?? 'full',
      'checks': results,
      'caps': caps,
      'engine_errors': errors,
      'destination_receipts': destinationReceipts.values.toList(),
      'unavailable_baseline_native_reads': unavailableNativeReads,
      'unavailable_native_schema_families': unavailableNativeFamilies,
      'outside_current_loop': [
        {
          'feature': 'Native X32 scenes, snippets and cues',
          'status': 'not_verified',
          'reason': 'The local numbered scene scenario is distinct from desk-native scene/snippet/cue slot operations; no native recall or hardware compatibility claim.',
        },
        {
          'feature': 'Native library and channel/FX/routing preset archives',
          'status': 'not_verified',
          'reason': 'Require schema-aware import/export, native slot provenance and physical-desk file compatibility checks.',
        },
        {
          'feature':
              'Recorder, USB multitrack audio and manual virtual soundcheck',
          'status': 'not_verified',
          'reason': 'The emulator command server has no physical USB audio device or capture/playback signal path; recording and soundcheck need independent OS/audio verification.',
        },
        {
          'feature': 'Automatic virtual-soundcheck routing',
          'status': 'not_verified',
          'reason': 'Remains subject to a verified provider hardware profile; no emulator-only automatic routing success claimed.',
        },
        {
          'feature':
              'LAN configuration and firmware-specific hardware functions',
          'status': 'not_verified',
          'reason': 'Not changed by this isolated loopback protocol run.',
        },
      ],
      'limits': [
        if (scenario == 'reconnect') 'Focused disconnect/reconnect scenario only; no channel/processing/FX inventory sweep in this invocation',
        if (scenario == 'routing') 'Focused routing scenario: twenty cycles of guarded FX source left/right routing and every observed output control, with typed receipts and independent readback',
        if (scenario == null) ...[
          'One representative per advertised family plus all final strip boundaries; not every strip/parameter permutation',
          'All slot-1 algorithm parameters and slot-5 first/last parameter schema checks; repeated interior slot-5 parameters inventoried',
        ],
        'Emulator evidence only; no UI, physical DSP/audio/USB/preamp/iOS proof',
        'Physical preamp behavior, native hardware archives, LAN reconfiguration, audio recording and virtual soundcheck require dedicated verification',
        'Run repository gates separately with emulators stopped',
      ],
    };
    await File(
      reportPath,
    ).writeAsString('${const JsonEncoder.withIndent('  ').convert(report)}\n');
    stdout.writeln('Evidence: $reportPath');
    exitCode = results.any((r) => r['status'] == 'fail')
        ? 1
        : results.any((r) => r['status'] == 'blocked')
        ? 2
        : 0;
  }
}

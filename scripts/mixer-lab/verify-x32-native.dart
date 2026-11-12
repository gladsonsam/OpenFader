// Manual verification against a disposable loopback fixture.
import 'native/archives.dart';
import 'native/client.dart';
import 'native/inventory.dart';
import 'native/presets.dart';
import 'native/show.dart';

Future<void> main(List<String> args) async {
  final probe = NativeProbe.fromArguments(args);
  try {
    await probe.connect();
    await probe.verifyInventory();
    await probe.verifyExportProvenance();
    if (probe.options['--only'] != 'export-provenance') {
      await probe.verifyShow();
      await probe.verifyPresets();
    }
    await probe.verifyArchives();
  } catch (error) {
    probe.checks.add({
      'check': 'Native verification environment',
      'status': 'fail',
      'reason': '$error',
    });
  } finally {
    await probe.cleanupShow();
    await probe.cleanupPresets();
    await probe.finish();
  }
}

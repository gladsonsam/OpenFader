// Manual command/wire inventory for the isolated X32 lab emulator.
import 'dart:io';

import 'osc.dart';
import 'x32/client.dart';
import 'x32/processing.dart';
import 'x32/routing.dart';
import 'x32/fx.dart';
import 'x32/groups.dart';
import 'x32/lifecycle.dart';
import 'x32/report.dart';

Future<void> main(List<String> args) async {
  final options = <String, String>{};
  for (var i = 0; i < args.length; i += 2) {
    require(i + 1 < args.length, 'Expected --option value');
    options[args[i]] = args[i + 1];
  }
  final uri = Uri.parse(options['--ws'] ?? '');
  require(
    uri.scheme == 'ws' && uri.host == '127.0.0.1',
    'Use isolated loopback lab',
  );
  final reportPath = options['--report'];
  require(reportPath != null, '--report is required');
  final scenario = options['--scenario'];
  final probe = X32Probe(uri, await WebSocket.connect(uri.toString()));
  try {
    await probe.attach();
    require(
      scenario == null || ['reconnect', 'routing'].contains(scenario),
      'Unknown --scenario; supported focused scenarios: reconnect, routing',
    );
    if (scenario == null) {
      await probe.processingPass();
      await probe.routingPass();
      await probe.fxPass();
      await probe.groupsPass();
      await probe.scenesAndMeters();
    } else if (scenario == 'routing') {
      await probe.routingStress();
    }
    if (scenario != 'routing') await probe.reconnectPass();
  } catch (e) {
    probe.results.add({
      'check': 'Full-probe environment',
      'status': 'fail',
      'reason': '$e',
    });
  } finally {
    await probe.finish(reportPath!, scenario);
  }
}

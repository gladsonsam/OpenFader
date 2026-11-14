import 'dart:io';

import 'osc.dart';
import 'wing/client.dart';
import 'wing/processing.dart';
import 'wing/lifecycle.dart';
import 'wing/routing.dart';
import 'wing/delay_filter.dart';
import 'wing/sends_groups.dart';
import 'wing/fx.dart';
import 'wing/processors.dart';

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
  final report = options['--report'];
  require(report != null, '--report required');
  final scenario = options['--scenario'];
  require(
    scenario == null ||
        [
          'processing',
          'source',
          'sends',
          'fx',
          'lifecycle',
          'processors',
        ].contains(scenario),
    'Unknown scenario',
  );
  final probe = WingProbe(uri, await WebSocket.connect(uri.toString()));
  try {
    await probe.attach();
    if (options['--expected-model'] case final expected?) {
      require(
        probe.latest['info']?['model'] == expected,
        'Expected model $expected, received ${probe.latest['info']?['model']}',
      );
    }
    if (scenario == 'processors') await probe.processorPass();
    if (scenario == null || scenario == 'processing')
      await probe.processingPass();
    if (scenario == null || scenario == 'source') {
      await probe.routingPass();
      await probe.delayFilterPass();
    }
    if (scenario == null || scenario == 'sends') await probe.sendsGroupsPass();
    if (scenario == null || scenario == 'fx') await probe.fxPass();
    if (scenario == null || scenario == 'lifecycle')
      await probe.lifecyclePass();
  } catch (e) {
    probe.results.add({
      'check': 'WING probe environment',
      'status': 'fail',
      'reason': '$e',
    });
  } finally {
    await probe.finish(report!, scenario);
  }
}

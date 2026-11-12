// Manual backend compatibility probe, not a widget/interaction test or CI gate.
import 'dart:async';
import 'dart:convert';
import 'dart:io';

import '../../app/lib/engine_protocol.dart';
import 'osc.dart';

Future<void> main(List<String> args) async {
  final options = <String, String>{};
  for (var i = 0; i < args.length; i += 2) {
    require(i + 1 < args.length, 'Expected --option value');
    options[args[i]] = args[i + 1];
  }
  final driver = options['--driver'];
  require(driver == 'wing' || driver == 'x32', 'Missing fixture adapter');
  final wing = driver == 'wing';
  final uri = Uri.parse(options['--ws'] ?? '');
  require(uri.scheme == 'ws' && uri.host == '127.0.0.1', 'Loopback lab only');
  final reference = OscReference(
    InternetAddress.loopbackIPv4,
    port: wing ? 2223 : 10023,
    wingReplies: wing,
  );
  final checks = <Map<String, Object?>>[];
  final errors = <Object?>[];
  final originals = <String, num>{};
  final events = <Map<String, dynamic>>[];
  Map<String, dynamic>? caps, info;
  var sequence = 0, infoSequence = 0;
  final values = <String, dynamic>{};
  final observed = <String, int>{};
  WebSocket? socket;
  StreamSubscription<dynamic>? subscription;
  Future<void> check(String name, Future<void> Function() run) async {
    try {
      await run();
      checks.add({'check': name, 'status': 'pass'});
      stdout.writeln('PASS $name');
    } catch (error) {
      checks.add({'check': name, 'status': 'fail', 'reason': '$error'});
      rethrow;
    }
  }

  try {
    socket = await WebSocket.connect(uri.toString())
        .timeout(const Duration(seconds: 10));
    subscription = socket.listen((raw) {
      if (raw is! String) return;
      final event = jsonDecode(raw) as Map<String, dynamic>;
      sequence++;
      if (event['type'] == 'caps' && event['driver'] == driver) caps = event;
      if (event['type'] == 'info' && event['driver'] == driver) {
        info = event;
        infoSequence = sequence;
      }
      if (event['type'] == 'sync_epoch') {
        values.clear();
        observed.clear();
      }
      if (event['type'] == 'params' && event['values'] is Map) {
        for (final entry in (event['values'] as Map).entries) {
          final key = '${event['ch']}/${entry.key}';
          values[key] = entry.value;
          observed[key] = sequence;
        }
      }
      if (event['type'] == 'error') errors.add(event);
      if (events.length < 100 && event['type'] != 'meters') events.add(event);
    });
    socket.add(Cmds.hello(false, true));
    socket.add(Cmds.probe('127.0.0.1'));
    await check(
      'Automatic identification, topology and capability attachment',
      () async {
        await until(() => caps != null && info != null, 'Provider attachment');
        require(
          (caps!['params'] as List).isNotEmpty,
          'No parameter descriptors',
        );
        require(
          (caps!['topology'] as List).isNotEmpty,
          'No topology descriptors',
        );
        require(
          (info!['model'] as String).isNotEmpty,
          'Missing model identity',
        );
      },
    );
    if (options['--expected-model'] case final expected?) {
      await check('Emulator model identity matches selected fixture', () async {
        require(
          info!['model'] == expected,
          'Expected $expected, observed ${info!['model']}',
        );
      });
    }
    await check('Connection command completed before control writes', () async {
      final start = sequence;
      socket!.add(Cmds.info());
      await until(() => infoSequence > start, 'Post-connect command barrier');
    });
    final inputs = (caps!['topology'] as List).cast<Map>().singleWhere(
      (f) => f['key'] == 'ch',
    );
    final first = inputs['first'] as int;
    final count = inputs['count'] as int;
    require(count > 1, 'Fixture must exercise distinct input boundaries');
    for (final index in [0, count - 1]) {
      final channel = first + index;
      final native = index + 1;
      final prefix = wing
          ? '/ch/$native'
          : '/ch/${native.toString().padLeft(2, '0')}';
      final fader = '$prefix/${wing ? 'fdr' : 'mix/fader'}';
      final mute = '$prefix/${wing ? 'mute' : 'mix/on'}';
      originals[fader] = await reference.read(fader);
      originals[mute] = await reference.read(mute);
      await check(
        'Input $native command -> independent OSC fader readback',
        () async {
          // Named fixture wire conversion; never a shared fallback law.
          const db = -60.0;
          final value = wing ? (db + 144.0) / 154.0 : 383.0 / 1023.0;
          socket!.add(
            Cmds.fader(
              channel,
              value,
              (inputs['strip_keys'] as List)[index] as String,
            ),
          );
          await reference.agrees(fader, wing ? db : value);
        },
      );
      await check('Input $native mute polarity in both directions', () async {
        for (final muted in [true, false]) {
          socket!.add(
            Cmds.mute(
              channel,
              muted,
              (inputs['strip_keys'] as List)[index] as String,
            ),
          );
          await reference.agrees(
            mute,
            wing ? (muted ? 1 : 0) : (muted ? 0 : 1),
          );
        }
      });
      await check(
        'Input $native external OSC change -> fresh engine observation',
        () async {
          final start = sequence;
          const wire = -36.0;
          final expected = wing ? (wire + 144.0) / 154.0 : 255.0 / 1023.0;
          await reference.write(fader, wing ? wire : expected);
          await reference.agrees(fader, wing ? wire : expected);
          await until(
            () =>
                (observed['$channel/fader'] ?? 0) > start &&
                values['$channel/fader'] is num &&
                ((values['$channel/fader'] as num) - expected).abs() < 0.00002,
            'External input $native observation',
          );
        },
      );
    }
    await check('No engine protocol errors', () async {
      require(errors.isEmpty, 'Engine errors: $errors');
    });
  } catch (error) {
    if (!checks.any((c) => c['status'] == 'fail')) {
      checks.add({
        'check': 'Verification environment',
        'status': 'fail',
        'reason': '$error',
      });
    }
  } finally {
    try {
      await check(
        'Restore changed emulator values with independent readback',
        () async {
          final failures = <String>[];
          for (final entry in originals.entries) {
            try {
              await reference.write(entry.key, entry.value);
              await reference.agrees(entry.key, entry.value);
            } catch (error) {
              failures.add('${entry.key}: $error');
            }
          }
          require(failures.isEmpty, failures.join('\n'));
        },
      );
    } catch (_) {
      // Failure is already recorded; still save evidence and close the socket.
    }
    await subscription?.cancel();
    await socket?.close();
    final report = {
      'time_utc': DateTime.now().toUtc().toIso8601String(),
      'driver': driver,
      'host': uri.toString(),
      'checks': checks,
      'info': info,
      'caps': caps,
      'engine_errors': errors,
      'initial_events': events,
      'limits': [
        'Protocol evidence only; no UI/hardware/audio/USB/simulator proof',
        'Only first and last input fader/mute and external change observation checked',
        'Run repository gates separately',
      ],
    };
    await File(
      options['--report']!,
    ).writeAsString('${const JsonEncoder.withIndent('  ').convert(report)}\n');
    if (checks.any((c) => c['status'] == 'fail')) exitCode = 1;
  }
}

// Manual, emulator-only compatibility loop. Not part of the CI test gate.
// Start the independent mixer-simulator emulator at commit
// unrecorded and an isolated OpenFader host.
// Then run with the Flutter SDK's dart executable:
// dart scripts/verify-x32-emulator.dart --emulator 127.0.0.3 \
//   --ws ws://127.0.0.1:18080/ws --report /tmp/x32-verification.json
// For a private desktop host, replace --ws with --descriptor <host.json>.
// Writes affect channel 32, physical output16, FX1 parameter1, the emulator
// clock selector and the 80-switch solo bank. Original OSC values are restored and
// independently checked afterward.
import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import '../app/lib/engine_protocol.dart';
import '../app/lib/core/channel_views.dart';
import '../app/lib/core/destination_controls.dart';

const referenceCommit = 'unrecorded';
const protocolRevision = 'protocol reference X32/M32 OSC 4.09-02 (2025-11-30)';
const protocolSource =
    'not recorded';
const protocolSha256 =
    'not recorded';

void require(bool condition, String reason) {
  if (!condition) throw StateError(reason);
}

Future<void> until(bool Function() ready, String what) async {
  final end = DateTime.now().add(const Duration(seconds: 15));
  while (!ready()) {
    if (DateTime.now().isAfter(end)) throw TimeoutException(what);
    await Future<void>.delayed(const Duration(milliseconds: 50));
  }
}

List<int> oscString(String value) {
  final bytes = [...utf8.encode(value), 0];
  while (bytes.length % 4 != 0) bytes.add(0);
  return bytes;
}

class OscReference {
  final InternetAddress address;
  OscReference(this.address);

  List<int> packet(String path, [num? value]) {
    final data = ByteData(4);
    if (value is int) {
      data.setInt32(0, value);
    } else if (value != null) {
      data.setFloat32(0, value.toDouble());
    }
    return [
      ...oscString(path),
      ...oscString(
        value == null
            ? ','
            : value is int
            ? ',i'
            : ',f',
      ),
      if (value != null) ...data.buffer.asUint8List(),
    ];
  }

  Future<void> write(String path, num value) async {
    final socket = await RawDatagramSocket.bind(
      InternetAddress.loopbackIPv4,
      0,
    );
    try {
      final bytes = packet(path, value);
      require(
        socket.send(bytes, address, 10023) == bytes.length,
        'OSC send failed',
      );
    } finally {
      socket.close();
    }
  }

  Future<num> read(String path) async {
    final socket = await RawDatagramSocket.bind(
      InternetAddress.loopbackIPv4,
      0,
    );
    try {
      final bytes = packet(path);
      require(
        socket.send(bytes, address, 10023) == bytes.length,
        'OSC query failed',
      );
      await for (final event in socket.timeout(const Duration(seconds: 2))) {
        if (event != RawSocketEvent.read) continue;
        final reply = socket.receive();
        if (reply == null ||
            reply.address.address != address.address ||
            reply.port != 10023)
          continue;
        final data = reply.data;
        final zero = data.indexOf(0);
        if (zero < 0 || utf8.decode(data.sublist(0, zero)) != path) continue;
        final tagsAt = ((zero + 4) ~/ 4) * 4;
        final tagsEnd = data.indexOf(0, tagsAt);
        require(tagsEnd >= 0, 'Malformed OSC type tag');
        final tags = utf8.decode(data.sublist(tagsAt, tagsEnd));
        final at = ((tagsEnd + 4) ~/ 4) * 4;
        require(data.length == at + 4, 'Unexpected OSC argument count');
        final word = ByteData.sublistView(data, at);
        if (tags == ',i') return word.getInt32(0);
        if (tags == ',f') {
          final value = word.getFloat32(0);
          require(value.isFinite, 'Non-finite OSC readback');
          return value;
        }
        throw StateError('Expected numeric OSC reply, received $tags');
      }
      throw StateError('OSC socket closed');
    } finally {
      socket.close();
    }
  }

  Future<void> agrees(String path, num expected) async {
    final end = DateTime.now().add(const Duration(seconds: 5));
    do {
      final actual = await read(path);
      if ((actual - expected).abs() <= 0.00002) return;
      await Future<void>.delayed(const Duration(milliseconds: 50));
    } while (DateTime.now().isBefore(end));
    throw StateError('$path did not independently read back $expected');
  }
}

Future<void> main(List<String> args) async {
  final options = <String, String>{};
  for (var i = 0; i < args.length; i += 2) {
    require(
      i + 1 < args.length && args[i].startsWith('--'),
      'Expected --option value',
    );
    options[args[i]] = args[i + 1];
  }
  final address = InternetAddress.tryParse(options['--emulator'] ?? '');
  require(
    address != null &&
        address.isLoopback &&
        address.type == InternetAddressType.IPv4,
    '--emulator must explicitly name a loopback IPv4 emulator',
  );
  final headers = <String, String>{};
  var endpoint = options['--ws'];
  if (options['--descriptor'] case final path?) {
    require(endpoint == null, 'Choose --ws or --descriptor');
    final descriptor = jsonDecode(await File(path).readAsString()) as Map;
    endpoint = Uri.parse(descriptor['endpoint'] as String)
        .replace(scheme: 'ws', path: '/ws')
        .toString();
    headers[HttpHeaders.authorizationHeader] = 'Bearer ${descriptor['token']}';
  }
  final uri = Uri.parse(endpoint ?? '');
  require(
    uri.scheme == 'ws' &&
        InternetAddress.tryParse(uri.host)?.isLoopback == true,
    'Use an isolated loopback OpenFader --ws endpoint or --descriptor',
  );

  final reference = OscReference(address!);
  final socket = await WebSocket.connect(uri.toString(), headers: headers);
  Map<String, dynamic>? caps, info;
  var controls = <DestinationControlVm>[];
  var sequence = 0, infoSequence = 0;
  final values = <int, Map<String, dynamic>>{};
  final valueSequence = <String, int>{};
  final errors = <String>[];
  final subscription = socket.listen((raw) {
    if (raw is! String) return;
    final event = jsonDecode(raw) as Map<String, dynamic>;
    sequence++;
    if (event['type'] == 'caps' && event['driver'] == 'x32') caps = event;
    if (event['type'] == 'info' && event['driver'] == 'x32') {
      info = event;
      infoSequence = sequence;
    }
    if (event['type'] == 'sync_epoch') values.clear();
    if (event['type'] == 'destination_controls' && event['controls'] is List) {
      controls = [
        for (final row in event['controls'] as List)
          if (row is Map)
            if (DestinationControlVm.fromJson(event['epoch'] as int, row)
                case final control?)
              control,
      ];
    }
    if (event['type'] == 'params' &&
        event['ch'] is int &&
        event['values'] is Map) {
      final ch = event['ch'] as int;
      for (final entry in (event['values'] as Map).entries) {
        (values[ch] ??= {})[entry.key as String] = entry.value;
        valueSequence['$ch/${entry.key}'] = sequence;
      }
    }
    if (event['type'] == 'error' && errors.length < 20)
      errors.add('${event['message'] ?? event['error']}');
  });
  final results = <Map<String, Object>>[];
  final originals = <String, num>{};
  Future<void> check(String name, Future<void> Function() run) async {
    try {
      await run();
      results.add({'check': name, 'status': 'pass'});
      stdout.writeln('PASS $name');
    } catch (error) {
      results.add({'check': name, 'status': 'fail', 'reason': '$error'});
      stdout.writeln('FAIL $name: $error');
    }
  }

  try {
    socket.add(Cmds.hello(false, true));
    socket.add(Cmds.connect('x32', address.address));
    await check('X32 identity and full capability attachment', () async {
      await until(
        () =>
            caps != null &&
            info != null &&
            (caps!['params'] as List).length > 10,
        'X32 attachment',
      );
      require(
        info!['model'] == 'X32',
        'This reference fixture must identify as X32',
      );
      require(
        (caps!['topology'] as List).fold<int>(
              0,
              (n, f) => n + (f['count'] as int),
            ) ==
            80,
        'Reference topology must cover 80 strips',
      );
    });
    await check(
      'Matrix sends and native EQ domains reach Flutter descriptors',
      () async {
        final metadata = {
          for (final p in caps!['params'] as List)
            p['id'] as String: Map<String, dynamic>.from(p as Map),
        };
        for (var slot = 17; slot <= 22; slot++) {
          require(
            metadata.containsKey('send${slot}_level'),
            'Matrix send $slot absent',
          );
        }
        for (final count in [6.0, 14.0]) {
          for (final band in [1, 4, 5, 6]) {
            final row = parameterPresentation(
              metadata['eq${band}_type'],
              (id) => id == 'x32:eq_type_count' ? count : null,
            );
            require(
              parameterChoices(row).length == count.toInt(),
              'EQ band $band native domain $count',
            );
          }
        }
        require(
          (caps!['fx']['types'] as List).length == 61,
          'Expected documented 61-effect catalogue',
        );
        for (final domain in {
          'insert_sel': 23,
          'x32:insert_pos': 2,
          'gate_key_src': 65,
          'comp_key_src': 65,
          'comp_knee': 6,
        }.entries) {
          require(
            parameterChoices(
                  parameterPresentation(metadata[domain.key], (_) => null),
                ).length ==
                domain.value,
            '${domain.key} must expose its complete documented native domain',
          );
        }
      },
    );
    const fader = '/ch/32/mix/fader', on = '/ch/32/mix/on';
    originals[fader] = await reference.read(fader);
    originals[on] = await reference.read(on);
    await check('Flutter fader command -> independent OSC readback', () async {
      socket.add(Cmds.fader(31, 1.0, 'ch/32'));
      await reference.agrees(fader, 1.0);
    });
    await check(
      'Flutter mute convention -> independent OSC readback',
      () async {
        socket.add(Cmds.mute(31, true, 'ch/32'));
        await reference.agrees(on, 0);
        socket.add(Cmds.mute(31, false, 'ch/32'));
        await reference.agrees(on, 1);
      },
    );
    await check('External OSC fader change -> engine observation', () async {
      final start = sequence;
      await reference.write(fader, 0.25);
      await until(
        () =>
            (valueSequence['31/fader'] ?? 0) > start &&
            values[31]?['fader'] == 0.25,
        'External change observation',
      );
    });
    await check(
      'Native input and dynamics selectors -> OSC readback',
      () async {
        for (final row in [
          (field: 'insert_sel', path: '/ch/32/insert/sel', value: 22),
          (field: 'x32:insert_pos', path: '/ch/32/insert/pos', value: 1),
          (field: 'gate_key_src', path: '/ch/32/gate/keysrc', value: 64),
          (field: 'comp_key_src', path: '/ch/32/dyn/keysrc', value: 64),
          (field: 'comp_knee', path: '/ch/32/dyn/knee', value: 5),
        ]) {
          originals[row.path] = await reference.read(row.path);
          socket.add(Cmds.param(31, row.field, row.value));
          // Knee is a normalized OSC float; selectors are OSC integers.
          await reference.agrees(
            row.path,
            row.field == 'comp_knee' ? 1.0 : row.value,
          );
        }
      },
    );
    const clock = '/-prefs/clockrate';
    originals[clock] = await reference.read(clock);
    await check(
      'Observed clock: 44.1 kHz and 48 kHz, without a shared default',
      () async {
        for (final entry in {1: 44100, 0: 48000}.entries) {
          await reference.write(clock, entry.key);
          await reference.agrees(clock, entry.key);
          final start = sequence;
          socket.add(Cmds.info());
          await until(
            () => infoSequence > start && info?['sample_rate'] == entry.value,
            'Clock ${entry.value}',
          );
        }
      },
    );
    await check('All-family solo descriptors and advertised clear', () async {
      List<DestinationControlVm> solos() =>
          controls.where((row) => row.surface == 'solo').toList();
      await until(
        () => solos().length == 80 && solos().every((row) => row.value != null),
        'Complete observed solo bank',
      );
      for (var index = 1; index <= 80; index++) {
        final path = '/-stat/solosw/${index.toString().padLeft(2, '0')}';
        originals[path] = await reference.read(path);
      }
      for (final (target, index) in [
        ('ch/01', 1),
        ('auxin/01', 33),
        ('fxrtn/01', 41),
        ('bus/01', 49),
        ('mtx/01', 65),
        ('main', 71),
        ('mainm', 72),
        ('vca/01', 73),
      ]) {
        final row = solos().singleWhere((row) => row.target == target);
        socket.add(
          Cmds.destinationParam('manual-solo-$index', row, 1, guard: [row]),
        );
        await reference.agrees(
          '/-stat/solosw/${index.toString().padLeft(2, '0')}',
          1,
        );
        await until(
          () => solos().any((r) => r.target == target && r.value == 1),
          'Observed $target solo',
        );
      }
      socket.add(Cmds.clearSolo());
      await until(
        () => solos().length == 80 && solos().every((row) => row.value == 0),
        'Advertised clear readback',
      );
      for (var index = 1; index <= 80; index++) {
        await reference.agrees(
          '/-stat/solosw/${index.toString().padLeft(2, '0')}',
          0,
        );
      }
    });
    await check(
      'Guarded output invert and delay controls -> OSC readback',
      () async {
        const target = 'outputs/main/16';
        List<DestinationControlVm> block() => controls
            .where((row) => row.surface == 'routing' && row.target == target)
            .toList();
        await until(
          () =>
              block().length == 4 && block().every((row) => row.value != null),
          'Complete output control block',
        );
        for (final (id, path) in [
          ('invert', '/outputs/main/16/invert'),
          ('delay_on', '/outputs/main/16/delay/on'),
          ('delay_time', '/outputs/main/16/delay/time'),
        ]) {
          originals[path] = await reference.read(path);
          final row = block().singleWhere((row) => row.id == id);
          final desired = id == 'delay_time'
              ? ((row.value! - 125.3).abs() < .1 ? 250.3 : 125.3)
              : 1.0 - row.value!;
          socket.add(
            Cmds.destinationParam(
              'manual-output-$id',
              row,
              desired,
              guard: block(),
            ),
          );
          await reference.agrees(
            path,
            id == 'delay_time' ? (desired - .3) / 499.7 : desired,
          );
          await until(
            () => block().any(
              (r) =>
                  r.id == id &&
                  r.value != null &&
                  (r.value! - desired).abs() < .11,
            ),
            'Observed output $id',
          );
        }
      },
    );
    // Leave FX type untouched; the reference power-on Hall has float par01.
    if (await reference.read('/fx/1/type') == 0) {
      const path = '/fx/1/par/01';
      originals[path] = await reference.read(path);
      await check(
        'Typed Hall FX parameter through Flutter command builder',
        () async {
          socket.add(Cmds.fxParam(1, 'par01', 0.25));
          await reference.agrees(path, 0.25);
        },
      );
    } else {
      results.add({
        'check': 'Typed Hall FX parameter',
        'status': 'not checked',
        'reason': 'Existing FX type preserved; use a fresh reference emulator',
      });
    }
  } finally {
    await check(
      'Restore all changed emulator values with independent readback',
      () async {
        for (final entry in originals.entries) {
          await reference.write(entry.key, entry.value);
          await reference.agrees(entry.key, entry.value);
        }
      },
    );
    await subscription.cancel();
    await socket.close();
    final report = {
      'time_utc': DateTime.now().toUtc().toIso8601String(),
      'protocol_reference': protocolRevision,
      'protocol_source': protocolSource,
      'expected_protocol_sha256': protocolSha256,
      'expected_emulator_commit': referenceCommit,
      'emulator_commit_checked_by_this_command': false,
      'emulator': address.address,
      'host': uri.toString(),
      'checks': results,
      'engine_errors': errors,
      'limits': [
        'Emulator and document evidence only; no hardware/audio/USB/iOS proof',
        'Run repository gates separately',
        'Use the pinned independent emulator; this command does not patch it',
      ],
    };
    final path = options['--report'] ?? '/tmp/openfader-x32-verification.json';
    await File(path)
        .writeAsString(const JsonEncoder.withIndent('  ').convert(report));
    stdout.writeln('Evidence: $path');
    if (results.any((r) => r['status'] == 'fail')) exitCode = 1;
  }
}

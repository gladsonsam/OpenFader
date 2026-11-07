import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:app/boot/remote_host.dart';
import 'package:app/engine_protocol.dart';

Future<void> main(List<String> args) async {
  if (args.length != 1) throw ArgumentError('Pass path to openfader-serve');
  final dataRoot = await Directory.systemTemp.createTemp('openfader-smoke-');
  Process? process;
  RemoteHost? first;
  RemoteHost? second;
  try {
    process = await Process.start(args.single, ['--desktop-host'],
        environment: {'OPENFADER_DATA_DIR': dataRoot.path});
    final descriptor = File('${dataRoot.path}/host.json');
    for (var i = 0; i < 200 && !await descriptor.exists(); i++) {
      await Future<void>.delayed(const Duration(milliseconds: 50));
    }
    final config = jsonDecode(await descriptor.readAsString()) as Map;
    final base = Uri.parse(config['endpoint'] as String);
    final admin = HttpClient();
    Future<Map<String, dynamic>> request(String action,
        [Map<String, dynamic>? body]) async {
      final req = await (body == null
          ? admin.getUrl(base.replace(path: '/api/remote/v1/$action'))
          : admin.postUrl(base.replace(path: '/api/remote/v1/$action')));
      req.headers.set('Authorization', 'Bearer ${config['token']}');
      if (body != null) {
        req.headers.contentType = ContentType.json;
        req.write(jsonEncode(body));
      }
      final response = await req.close();
      final text = await utf8.decoder.bind(response).join();
      if (response.statusCode != 200) {
        throw StateError('Admin HTTP ${response.statusCode}');
      }
      return Map<String, dynamic>.from(jsonDecode(text) as Map);
    }

    final enabled = await request('enable', {'ip': '127.0.0.1', 'port': 0});
    final address = 'https://127.0.0.1:${enabled['port']}';
    Future<void> rejects(Future<void> Function() action) async {
      var failed = false;
      try {
        await action();
      } catch (_) {
        failed = true;
      }
      if (!failed) throw StateError('Expected rejection');
    }

    final open = await RemoteHost.inspect(address);
    if (open.passwordRequired) throw StateError('Unexpected server password');
    first = await RemoteHost.connect(open);
    await first.recorderRequest('state');
    await first.disconnect();
    await request('password', {'password': 'smoke-password'});
    final secured =
        await RemoteHost.inspect(address, trustedFingerprint: open.fingerprint);
    if (!secured.passwordRequired) {
      throw StateError('Missing password challenge');
    }
    await rejects(() async {
      await RemoteHost.connect(secured, password: 'wrong');
    });
    final wrong = RemoteServer(secured.endpoint, '0' * 64, null, true);
    await rejects(() async {
      await RemoteHost.connect(wrong, password: 'smoke-password');
    });
    await Future<void>.delayed(const Duration(milliseconds: 1100));
    first = await RemoteHost.connect(secured, password: 'smoke-password');
    second = await RemoteHost.connect(secured, password: 'smoke-password');
    await first.boot();
    await second.boot();
    await first.cmd(first.clientId, Cmds.connectSim());
    await Future<void>.delayed(const Duration(milliseconds: 500));
    await second.poll(second.clientId, 100000);
    await first.cmd(first.clientId, Cmds.mute(0, true));
    await Future<void>.delayed(const Duration(milliseconds: 500));
    final events = await second.poll(second.clientId, 100000);
    if (!events.any((e) {
      final event = jsonDecode(e);
      return event is Map &&
          event['type'] == 'param' &&
          event['ch'] == 0 &&
          event['field'] == 'mute' &&
          event['value'] == true;
    })) {
      throw StateError('No mute event received by second client');
    }
    if (!events.any((text) {
      final event = jsonDecode(text);
      return event is Map &&
          event['type'] == 'state' &&
          event['strips'] is List &&
          (event['strips'] as List).any((strip) =>
              strip is Map && strip['ch'] == 0 && strip['muted'] == true);
    })) {
      throw StateError('Second client did not observe muted state');
    }
    await request('revoke', {'client_id': first.clientId});
    await Future<void>.delayed(const Duration(milliseconds: 300));
    await rejects(() async {
      await first!.recorderRequest('state');
    });
    await rejects(() async {
      await first!.cmd(first.clientId, Cmds.info());
    });
    await second.disconnect();
    await request('status');
    await request('disable', {});
    await rejects(() async {
      final socket =
          await Socket.connect(first!.endpoint.host, first.endpoint.port);
      socket.destroy();
    });
    admin.close(force: true);
    stdout.writeln('Transport smoke passed');
  } finally {
    await first?.disconnect();
    await second?.disconnect();
    process?.kill();
    if (process != null) await process.exitCode;
    await dataRoot.delete(recursive: true);
  }
}

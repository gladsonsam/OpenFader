import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:app/boot/remote_host.dart';
import 'package:app/engine_protocol.dart';

Future<void> main(List<String> args) async {
  if (args.length != 1) throw ArgumentError('Pass path to openfader-serve');
  final home = await Directory.systemTemp.createTemp('openfader-smoke-');
  Process? process;
  RemoteHost? first;
  RemoteHost? second;
  try {
    process = await Process.start(args.single, ['--desktop-host'],
        environment: {'HOME': home.path});
    final descriptor = File(
        '${home.path}/Library/Application Support/OpenFader/Recorder/host.json');
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

    await request('enable', {'ip': '127.0.0.1'});
    Future<String> link() async {
      final p = await request('pairing', {});
      return jsonEncode({
        'endpoint': 'https://127.0.0.1:${p['port']}',
        'fingerprint': p['fingerprint'],
        'secret': p['secret']
      });
    }

    Future<void> rejects(Future<void> Function() action) async {
      var failed = false;
      try {
        await action();
      } catch (_) {
        failed = true;
      }
      if (!failed) throw StateError('Expected rejection');
    }

    final original = await link();
    final wrong = Map<String, dynamic>.from(jsonDecode(original) as Map);
    wrong['fingerprint'] = '0' * 64;
    await rejects(() async {
      await RemoteHost.pair(PairingLink.parse(jsonEncode(wrong)));
    });
    first = await RemoteHost.pair(PairingLink.parse(original));
    await rejects(() async {
      await RemoteHost.pair(PairingLink.parse(original));
    });
    await first.recorderRequest('state');
    second = await RemoteHost.pair(PairingLink.parse(await link()));
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
    await home.delete(recursive: true);
  }
}

import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:math' as math;

import 'tuple.dart';

import '../../../app/lib/engine_protocol.dart';
import '../osc.dart';
import 'client.dart';

extension WingProcessors on WingProbe {
  Future<Map> processorNode(String path) async {
    final id = 'processor-node-${sequence + 1}';
    socket.add(Cmds.consoleControls(id, path));
    await until(
      () => latest['console_controls']?['request_id'] == id,
      'Native processor descriptor $path',
    );
    final event = latest['console_controls']!;
    require(event['error'] == null, 'Native descriptor error: $event');
    return event;
  }

  Future<void> processorWrite(String root, String leaf, Object value) async {
    final event = await processorNode(root);
    final row = (event['node']['items'] as List).cast<Map>().singleWhere(
      (r) => r['path'] == '$root/$leaf',
    );
    require(
      row['read_only'] == false && row['revision'] != null,
      'Processor field unavailable: $row',
    );
    final desired = row['kind'] == 'boolean' && value is num
        ? value != 0
        : value;
    final id = 'processor-write-${sequence + 1}';
    socket.add(
      Cmds.consoleControlSet(
        id,
        event['epoch'] as int,
        row['path'] as String,
        desired,
        row['revision'] as Object,
      ),
    );
    await until(
      () => latest['console_control_result']?['request_id'] == id,
      'Native processor write $root/$leaf',
    );
    if (latest['console_control_result']?['status'] != 'verified') {
      Object? independent;
      try {
        independent = await native.read('$root/$leaf');
      } catch (error) {
        independent = '$error';
      }
      throw StateError(
        'Processor write $root/$leaf requested $desired; independently read $independent: ${latest['console_control_result']}',
      );
    }
  }

  Future<Object> processorRead(String root, Map field) async {
    final path = '$root/${field['name']}';
    if (field['type'] == 'integer') {
      return readInteger(path, field['minint'] as int, field['maxint'] as int);
    }
    return native.read(path);
  }

  Object processorAlternate(Map field, Object current) {
    if (field['items'] case final List choices) {
      final values = choices
          .cast<Map>()
          .map((c) => c['item'] as Object)
          .toList();
      return same(values.last, current) ? values.first : values.last;
    }
    final lo = field['minfloat'] ?? field['minint'];
    final hi = field['maxfloat'] ?? field['maxint'];
    require(lo is num && hi is num, 'Unknown processor domain: $field');
    return same(hi, current) ? lo as Object : hi as Object;
  }

  bool processorAgrees(Object? a, Object? b) {
    if (a is! num || b is! num) return a == b;
    final scale = math.max(a.abs(), b.abs());
    return (a - b).abs() <=
        math.max(0.000025, 4 * 1.1920928955078125e-7 * scale);
  }

  Future<Object> processorExpected(String root, Map field, Object value) async {
    if (field['type'] != 'linear float' && field['type'] != 'log float')
      return value;
    final dd = await readWingParameterDefinition(
      native,
      root,
      field['name'] as String,
    );
    results.add({
      'check': 'Independent processor domain $root/${field['name']}',
      'status': 'observation',
      'definition': dd,
    });
    final lo = (dd['min'] as num).toDouble(),
        hi = (dd['max'] as num).toDouble();
    final steps = dd['steps'] as int;
    if (steps <= 1 || lo == hi) return value;
    final n = (value as num).toDouble();
    final fraction = dd['kind'] == 'log'
        ? math.log(n / lo) / math.log(hi / lo)
        : (n - lo) / (hi - lo);
    final index = (fraction * (steps - 1)).round();
    return dd['kind'] == 'log'
        ? lo * math.pow(hi / lo, index / (steps - 1))
        : lo + (hi - lo) * index / (steps - 1);
  }

  Future<(Object, Object)> processorDifferent(
    String root,
    Map field,
    Object current,
  ) async {
    var value = processorAlternate(field, current);
    if (field['type'] == 'linear float' || field['type'] == 'log float') {
      final dd = await readWingParameterDefinition(
        native,
        root,
        field['name'] as String,
      );
      final a = dd['min'] as num, b = dd['max'] as num;
      final bounded = (value as num).clamp(math.min(a, b), math.max(a, b));
      if (value != bounded) {
        results.add({
          'check': 'Reference/live domain boundary $root/${field['name']}',
          'status': 'observation',
          'reference_endpoint': value,
          'live_endpoint': bounded,
          'reason': 'Exercise the independently observed current native domain; retain the older reference discrepancy.',
        });
        value = bounded;
      }
    }
    var expected = await processorExpected(root, field, value);
    if (processorAgrees(expected, current) &&
        (field['type'] == 'linear float' || field['type'] == 'log float')) {
      final dd = await readWingParameterDefinition(
        native,
        root,
        field['name'] as String,
      );
      for (final endpoint in [dd['min']!, dd['max']!]) {
        final candidate = await processorExpected(root, field, endpoint);
        if (!processorAgrees(candidate, current)) {
          value = endpoint;
          expected = candidate;
          break;
        }
      }
    }
    return (value, expected);
  }

  Future<void> processorSceneCommand(String cmd) async {
    final start = errors.length, marker = helloSequence;
    socket.add(cmd);
    socket.add(Cmds.hello(false, true));
    final deadline = DateTime.now().add(const Duration(seconds: 90));
    while (helloSequence <= marker && DateTime.now().isBefore(deadline)) {
      await Future<void>.delayed(const Duration(milliseconds: 50));
    }
    require(
      helloSequence > marker,
      'Native scene command did not finish within 90 seconds',
    );
    require(
      errors.length == start,
      'Native scene command failed: ${errors.sublist(start)}',
    );
  }

  Future<void> processorPass() async {
    final reference = jsonDecode(
      await File.fromUri(
        Platform.script.resolve('wing/processors-reference.json'),
      ).readAsString(),
    ) as Map;
    final domains = reference['domains'] as Map;
    final selected = Platform.environment['OPENFADER_LAB_PROCESSOR_MODELS']
        ?.split(',')
        .toSet();
    final known = domains.values
        .expand((models) => (models as List).cast<Map>())
        .map((m) => m['id'])
        .toSet();
    require(
      selected == null || (selected.isNotEmpty && known.containsAll(selected)),
      'Unknown processor model filter',
    );
    results.add({
      'check': 'Native processor probe selection',
      'status': 'observation',
      'models': selected?.toList() ?? 'all independently inventoried models',
      'scope': 'All fields receive independent OSC writes/readback; representative field kinds and model selection use OpenFader commands; full local scenes prove all captured fields.',
    });
    final originalsByDomain = <String, Map<String, Object>>{};
    for (final entry in domains.entries) {
      final root = '/ch/1/${entry.key}';
      final model = await remember('$root/mdl');
      final row = (entry.value as List).cast<Map>().singleWhere(
        (r) => r['id'] == model,
      );
      final values = <String, Object>{'mdl': model};
      for (final field in (row['fields'] as List).cast<Map>()) {
        final value = await processorRead(root, field);
        final path = '$root/${field['name']}';
        originals[path] = value;
        if (field['type'] == 'integer') {
          integerDomains[path] = (
            field['minint'] as int,
            field['maxint'] as int,
          );
        }
        values[field['name'] as String] = value;
      }
      originalsByDomain[entry.key as String] = values;
    }
    results.add({
      'check': 'Independent native processor facts',
      'status': 'observation',
      'source': reference['source'],
      'git_blob': reference['git_blob'],
    });
    for (final entry in domains.entries) {
      final root = '/ch/1/${entry.key}';
      for (final model in (entry.value as List).cast<Map>()) {
        if (selected != null && !selected.contains(model['id'])) continue;
        await check(
          '${entry.key}/${model['id']} native parameters and cross-model scene recall',
          () async {
            const number = 999;
            require(
              !(latest['scenes']?['scenes'] as List? ?? []).any(
                (s) => s['n'] == number,
              ),
              'Reserved scene slot is occupied',
            );
            var saved = false;
            try {
              await processorWrite(root, 'mdl', model['id'] as String);
              await native.agrees('$root/mdl', model['id'] as String);
              final descriptors =
                  (await processorNode(root))['node']['items'] as List;
              final fields = (model['fields'] as List).cast<Map>().map((field) {
                final candidates = [
                  field['name'],
                  ...(field['aliases'] as List? ?? []),
                ];
                final observed = candidates
                    .where(
                      (name) => descriptors.cast<Map>().any(
                        (row) => row['path'] == '$root/$name',
                      ),
                    )
                    .toList();
                require(
                  observed.length == 1,
                  'Missing or ambiguous documented native field $field',
                );
                return <String, dynamic>{
                  ...field.cast<String, dynamic>(),
                  'name': observed.single,
                };
              }).toList();
              final captured = <String, Object>{};
              final browserKinds = <String>{};
              for (final field in fields) {
                final old = await processorRead(root, field);
                final (value, expected) = await processorDifferent(
                  root,
                  field,
                  old,
                );
                final common = ['on', 'mix', 'gain'].contains(field['name']);
                final type = '${field['type']}';
                if ((!common && browserKinds.add(type)) ||
                    field['name'] == 'hld') {
                  await processorWrite(root, field['name'] as String, value);
                } else {
                  await native.write('$root/${field['name']}', value);
                }
                final read = await processorRead(root, field);
                require(
                  processorAgrees(read, expected),
                  'Native endpoint disagrees: $field = $read, expected native grid $expected from requested $value',
                );
                captured[field['name'] as String] = read;
              }
              await processorSceneCommand(
                Cmds.sceneSave(number, 'WING ${entry.key} ${model['id']}'),
              );
              saved = true;
              for (final field in fields) {
                final (next, expected) = await processorDifferent(
                  root,
                  field,
                  captured[field['name']]!,
                );
                await native.write('$root/${field['name']}', next);
                require(
                  processorAgrees(await processorRead(root, field), expected),
                  'Unable to stage different processor value',
                );
              }
              final other = (entry.value as List).cast<Map>().firstWhere(
                (r) => r['id'] != model['id'],
              );
              await processorWrite(root, 'mdl', other['id'] as String);
              await native.agrees('$root/mdl', other['id'] as String);
              await processorSceneCommand(Cmds.sceneRecall(number));
              await native.agrees('$root/mdl', model['id'] as String);
              for (final field in fields) {
                require(
                  processorAgrees(
                    await processorRead(root, field),
                    captured[field['name']],
                  ),
                  'Scene lost native ${field['name']} for ${model['id']}',
                );
              }
            } finally {
              if (saved) await processorSceneCommand(Cmds.sceneDelete(number));
              // Retire each disposable model before the next independent case.
              final initial = originalsByDomain[entry.key]!;
              await native.write('$root/mdl', initial['mdl']!);
              await native.agrees('$root/mdl', initial['mdl']!);
              for (final value in initial.entries.where(
                (e) => e.key != 'mdl',
              )) {
                await native.write('$root/${value.key}', value.value);
              }
            }
          },
        );
      }
    }
  }
}

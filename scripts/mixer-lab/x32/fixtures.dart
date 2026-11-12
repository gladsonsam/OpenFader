import '../osc.dart' show same;

class Sample {
  final String leaf;
  final Object value, wire;
  Sample(this.leaf, this.value, this.wire);
}

// Explicit X32 fixture facts, independent of the production mapping implementation.
String rootFor(String key) {
  if (key == 'main') return '/main/st';
  if (key == 'mainm') return '/main/m';
  final p = key.split('/');
  if (p[0] == 'dca') return '/dca/${int.parse(p[1])}';
  return '/$key';
}

Sample? sampleFor(String id, Map p, String family, Object original) {
  const leaves = {
    'fader': 'mix/fader',
    'mute': 'mix/on',
    'name': 'config/name',
    'color': 'config/color',
    'icon': 'config/icon',
    'pan': 'mix/pan',
    'lr_assign': 'mix/st',
    'x32:mono_assign': 'mix/mono',
    'x32:mono_level': 'mix/mlevel',
    'trim': 'preamp/trim',
    'polarity': 'preamp/invert',
    'delay_on': 'delay/on',
    'delay_ms': 'delay/time',
    'hpf_on': 'preamp/hpon',
    'hpf_freq': 'preamp/hpf',
    'hpf_slope': 'preamp/hpslope',
    'eq_on': 'eq/on',
    'insert_on': 'insert/on',
    'insert_sel': 'insert/sel',
    'x32:insert_pos': 'insert/pos',
    'automix_group': 'automix/group',
    'automix_weight': 'automix/weight',
    'gate_on': 'gate/on',
    'gate_mode': 'gate/mode',
    'gate_thr': 'gate/thr',
    'gate_range': 'gate/range',
    'gate_att': 'gate/attack',
    'gate_hold': 'gate/hold',
    'gate_rel': 'gate/release',
    'gate_key_src': 'gate/keysrc',
    'gate_filter_on': 'gate/filter/on',
    'gate_filter_type': 'gate/filter/type',
    'gate_filter_freq': 'gate/filter/f',
    'comp_on': 'dyn/on',
    'comp_mode': 'dyn/mode',
    'comp_det': 'dyn/det',
    'comp_env': 'dyn/env',
    'comp_thr': 'dyn/thr',
    'comp_ratio': 'dyn/ratio',
    'comp_knee': 'dyn/knee',
    'comp_makeup': 'dyn/mgain',
    'comp_att': 'dyn/attack',
    'comp_hold': 'dyn/hold',
    'comp_rel': 'dyn/release',
    'comp_mix': 'dyn/mix',
    'comp_auto': 'dyn/auto',
    'comp_key_src': 'dyn/keysrc',
    'comp_filter_on': 'dyn/filter/on',
    'comp_filter_type': 'dyn/filter/type',
    'comp_filter_freq': 'dyn/filter/f',
  };
  String? leaf = leaves[id];
  if (family == 'dca' && (id == 'fader' || id == 'mute'))
    leaf = id == 'fader' ? 'fader' : 'on';
  final eq = RegExp(r'^eq(\d+)_(gain|freq|q|type)$').firstMatch(id);
  if (eq != null)
    leaf =
        'eq/${eq[1]}/${{'gain': 'g', 'freq': 'f', 'q': 'q', 'type': 'type'}[eq[2]]}';
  final send = RegExp(r'^send(\d+)_(level|on|pan|tap)$').firstMatch(id);
  if (send != null) {
    final n = int.parse(send[1]!);
    final native = n > 16 ? n - 16 : n;
    leaf =
        'mix/${native.toString().padLeft(2, '0')}/${send[2] == 'tap' ? 'type' : send[2]}';
  }
  final dca = RegExp(r'^dca(\d+)_assign$').firstMatch(id);
  final mg = RegExp(r'^_mg(\d+)$').firstMatch(id);
  if (dca != null || mg != null) {
    final bit = 1 << (int.parse((dca ?? mg)![1]!) - 1);
    final mask = (original as num).toInt();
    return Sample(
      dca != null ? 'grp/dca' : 'grp/mute',
      mask & bit == 0,
      mask ^ bit,
    );
  }
  if (leaf == null) return null;
  if (p['kind'] == 'text') return Sample(leaf, 'LAB CHECK', 'LAB CHECK');
  if (p['kind'] == 'bool') {
    final wire = original == 0 ? 1 : 0;
    return Sample(leaf, id == 'mute' ? wire == 0 : wire == 1, wire);
  }
  if (p['kind'] == 'enum') {
    var max = (p['max'] as num).toInt();
    // This pinned emulator implements one six-choice EQ table for every
    // family. Wider hardware-native options remain explicit blocked coverage.
    if (eq != null) max = 5;
    final value = original == max ? (p['min'] as num).toInt() : max;
    return Sample(leaf, value, value);
  }
  if (id == 'fader') {
    final desired = same(original, 383.0 / 1023.0)
        ? 639.0 / 1023.0
        : 383.0 / 1023.0;
    return Sample(leaf, desired, desired);
  }
  if (id == 'comp_ratio')
    return Sample(leaf, original == 11 ? 1.1 : 100.0, original == 11 ? 0 : 11);
  if (id == 'x32:mono_level' || send?[2] == 'level') {
    return Sample(
      leaf,
      original == 0 ? 10.0 : -90.0,
      original == 0 ? 1.0 : 0.0,
    );
  }
  final reversed = eq?[2] == 'q';
  final high = original == (reversed ? 1.0 : 0.0);
  return Sample(
    leaf,
    high ? p['max'] as num : p['min'] as num,
    high ? (reversed ? 0.0 : 1.0) : (reversed ? 1.0 : 0.0),
  );
}

bool applicable(String id, String family) {
  if ((family == 'mainm' && id == 'pan') ||
      (family == 'fxrtn' &&
          [
            'trim',
            'polarity',
            'gain',
            'phantom',
            'source',
            'x32:preamp_state',
          ].contains(id)))
    return false;
  return true;
}

String? routePath(String id) {
  final p = id.split('/');
  if (p[0] == 'input') return '/${p.sublist(1).join('/')}/config/source';
  if (p[0] == 'outputs') return '/$id/src';
  if (p[0] == 'userrout') return '/config/$id';
  if (p[0] == 'fx' && p[1] == 'source')
    return '/fx/${int.parse(p[2])}/source/${p[3]}';
  if (p[0] != 'routing') return null;
  if (p[1] == 'mode') return '/config/routing/routswitch';
  final slot = int.parse(p[2]);
  if (p[1] == 'inaux' || p[1] == 'playaux')
    return '/config/routing/${p[1] == 'inaux' ? 'IN' : 'PLAY'}/AUX';
  final name = {
    'in': 'IN',
    'play': 'PLAY',
    'aes50a': 'AES50A',
    'aes50b': 'AES50B',
    'card': 'CARD',
  }[p[1]];
  if (name != null)
    return '/config/routing/$name/${(slot - 1) * 8 + 1}-${slot * 8}';
  if (p[1] == 'out-a')
    return '/config/routing/OUT/${slot == 1 ? '1-4' : '9-12'}';
  if (p[1] == 'out-b')
    return '/config/routing/OUT/${slot == 1 ? '5-8' : '13-16'}';
  return null;
}

const fx58 = [
  'GEQ2',
  'GEQ',
  'TEQ2',
  'TEQ',
  'DES2',
  'DES',
  'P1A',
  'P1A2',
  'PQ5',
  'PQ5S',
  'WAVD',
  'LIM',
  'FAC',
  'FAC1M',
  'FAC2',
  'LEC',
  'LEC2',
  'ULC',
  'ULC2',
  'ENH2',
  'ENH',
  'EXC2',
  'EXC',
  'IMG',
  'EDI',
  'SON',
  'AMP2',
  'AMP',
  'DRV2',
  'DRV',
  'PHAS',
  'FILT',
  'PAN',
  'SUB',
];

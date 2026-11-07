// Explicit, opt-in offline fixture for repeatable real UI captures.
import 'dart:async';
import '../engine.dart';
import '../state/app_store.dart';

Future<void> stageMarketingFixture(AppStore store) async {
  for (var attempt = 0; attempt < 100; attempt++) {
    if (store.engine.connected && store.engine.driver == 'sim') break;
    await Future<void>.delayed(const Duration(milliseconds: 100));
  }
  if (!store.engine.connected || store.engine.driver != 'sim') return;
  const names = ['Kick', 'Snare', 'Hat', 'Tom 1', 'Tom 2', 'Floor',
    'OH L', 'OH R', 'Bass', 'Gtr L', 'Gtr R', 'Keys L'];
  for (var ch = 0; ch < names.length; ch++) {
    if (!store.engine.connected || store.engine.driver != 'sim') return;
    for (final entry in <String, Object>{
      'name': names[ch], 'eq_on': true, 'hpf_on': true,
      'hpf_freq': ch == 0 ? 35.0 : 85.0,
      'eq1_freq': 120.0, 'eq1_gain': 2.0 + ch % 3,
      'eq2_freq': 360.0, 'eq2_gain': -4.5 + ch % 2,
      'eq3_freq': 3200.0, 'eq3_gain': 2.5,
      'eq4_freq': 9500.0, 'eq4_gain': 1.5,
      'comp_on': true, 'comp_thr': -22.0 + ch,
      'comp_ratio': 3.0 + ch % 3, 'comp_att': 15.0 + ch,
      'comp_rel': 120.0, 'pan': ch < 6 ? 0.0 : (ch.isEven ? -0.3 : 0.3),
      'send7_level': -14.0 - ch,
    }.entries) {
      await store.sendCommand(Cmds.param(ch, entry.key, entry.value));
    }
  }
  await store.sendCommand(Cmds.captureScene('Evening set', '01 Soundcheck'));
  await store.sendCommand(Cmds.captureScene('Evening set', '02 Opening song'));
}

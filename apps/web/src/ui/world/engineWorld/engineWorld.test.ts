// W2a EngineWorld tests: ReplayTransport driving the golden caps/params/state
// frames, param pending/confirm/timeout/revert/coalescing with an injected
// clock, focus MRU/debounce, binary meter decode without reallocation.
// Run: node --experimental-strip-types engineWorld/engineWorld.test.ts

import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { EngineWorld } from './engineWorld.ts';
import { ReplayTransport } from '../transport.ts';

const here = dirname(fileURLToPath(import.meta.url));
const GOLDEN = join(here, '..', '..', '..', '..', '..', '..', 'crates', 'engine', 'tests', 'golden');
const gold = (f: string): string => readFileSync(join(GOLDEN, f), 'utf8');

let passed = 0;
let failed = 0;
const failures: string[] = [];
function ok(cond: boolean, name: string, extra?: string): void {
  if (cond) passed++;
  else {
    failed++;
    failures.push(name + (extra ? ' :: ' + extra : ''));
  }
}
function approx(a: number, b: number, eps = 1e-4): boolean {
  return Math.abs(a - b) <= eps;
}
void approx;

interface Ctl {
  now: number;
  timers: Array<{ fn: () => void }>;
}
function setup() {
  const ctl: Ctl = { now: 1000, timers: [] };
  const tr = new ReplayTransport();
  const world = new EngineWorld(tr, {
    now: () => ctl.now,
    perfNow: () => ctl.now,
    setTimer: (fn) => {
      const t = { fn };
      ctl.timers.push(t);
      return t;
    },
    clearTimer: (t) => {
      const i = ctl.timers.indexOf(t as { fn: () => void });
      if (i >= 0) ctl.timers.splice(i, 1);
    },
  });
  const runTimers = (): void => {
    for (const t of ctl.timers.splice(0)) t.fn();
  };
  return { ctl, tr, world, runTimers };
}

function feedAttach(tr: ReplayTransport): void {
  tr.feedText(gold('you.json'));
  tr.feedText(gold('state.json'));
  tr.feedText(gold('caps.json'));
  tr.feedText(gold('params.json'));
  tr.feedText(gold('param.json'));
}

// Full-ish caps for the channel-view tests (ranges mirror the catalogue).
function richCaps(driver = 'sim'): string {
  const rows: unknown[] = [];
  const f = (id: string, min: number, max: number, unit = '', def = 0) => ({
    id, kind: 'float', scope: 'channel', scopes: ['in'], min, max, unit, default: def, label: id,
  });
  const b = (id: string, def = false) => ({
    id, kind: 'bool', scope: 'channel', scopes: ['in'], min: null, max: null, unit: '', default: def, label: id,
  });
  const e = (id: string, min: number, max: number, def = 0) => ({
    id, kind: 'enum', scope: 'channel', scopes: ['in'], min, max, unit: '', default: def, label: id,
  });
  rows.push(
    f('fader', 0, 1, '', 0.75), b('mute'), { id: 'name', kind: 'text', scope: 'channel', scopes: ['in'], min: null, max: null, unit: '', default: '', label: 'name' },
    e('color', 0, 15, 2), b('phantom'), f('gain', -12, 60, 'dB', 20), f('trim', -18, 18, 'dB', 0),
    b('polarity'), b('delay_on'), f('delay_ms', 0, 100, 'ms', 0), f('pan', -100, 100, 'L/R', 0),
    b('hpf_on'), f('hpf_freq', 20, 400, 'Hz', 80), e('hpf_slope', 0, 2, 0),
    b('eq_on'), f('eq1_gain', -15, 15, 'dB', 0), f('eq1_freq', 20, 20000, 'Hz', 100), f('eq1_q', 0.3, 10, '', 1), e('eq1_type', 0, 5, 1),
    f('eq5_gain', -15, 15, 'dB', 0), f('eq5_freq', 20, 20000, 'Hz', 16000), f('eq5_q', 0.3, 10, '', 1), e('eq5_type', 0, 5, 2),
    b('gate_on'), f('gate_thr', -80, 0, 'dB', -50), f('gate_range', 0, 60, 'dB', 60), f('gate_att', 0, 120, 'ms', 1), f('gate_hold', 0, 500, 'ms', 50), f('gate_rel', 5, 1000, 'ms', 100),
    b('comp_on'), f('comp_thr', -60, 0, 'dB', -20), f('comp_ratio', 1.1, 100, '', 2), f('comp_knee', 0, 5, '', 3), f('comp_makeup', 0, 24, 'dB', 0),
    f('comp_att', 0, 300, 'ms', 10), f('comp_hold', 0, 500, 'ms', 20), f('comp_rel', 10, 1000, 'ms', 100),
    f('send1_level', -90, 10, 'dB', -90), e('send1_tap', 0, 1, 1), f('send1_pan', -100, 100, 'L/R', 0), b('send1_on', true),
    f('send2_level', -90, 10, 'dB', -90), e('send2_tap', 0, 1, 1), b('send2_on', true),
  );
  return JSON.stringify({
    type: 'caps', driver, params: rows, unsupported: ['usbreturn'],
    topology: [{ key: 'ch', kind: 'in', label: 'Ch 1-32', first: 0, count: 32, name_prefix: 'Ch', color_default: 2, params_scope: ['in'] }],
    meter_taps: ['in', 'post', 'gr_gate', 'gr_comp'],
  });
}

// ---- attach snapshot: golden caps/params/state ----

{
  const { tr, world } = setup();
  ok(world.capsReady === false, 'caps not ready before frames');
  ok(world.setFader(0, 0.5) === false, 'writes refused before caps (honest unknown)');
  ok(world.status(0, 'gain') === 'unknown', 'pre-caps status unknown');
  feedAttach(tr);
  ok(world.capsReady, 'caps ready after frame');
  ok(world.selfId === 'client-1', 'self id from you');
  ok(world.driver === 'sim', 'driver from state/caps');
  ok(world.stripCount === 32, 'stripCount from golden topology', String(world.stripCount));
  ok(world.banks.length === 2 && world.banks[0]!.key === 'ch1' && world.banks[1]!.key === 'ch17', 'golden topology banks');
  ok(world.strip(0).name === 'CH01' || world.strip(0).name === 'CH01', 'strip 0 keeps state name');
  ok(world.supports('gain', 0) === true, 'gain supported on ch 0');
  ok(world.supports('usbreturn', 0) === false, 'unsupported via caps');
  ok(world.supports('nope', 0) === false, 'unknown id unsupported');
  ok(world.param(3, 'gain') === 12.5, 'params frame value');
  ok(world.status(3, 'gain') === 'known', 'confirmed status known');
  ok(world.status(2, 'gain') === 'known', 'param frame confirms too');
  ok(world.status(0, 'gain') === 'unknown', 'unseen strip unknown');
  ok(world.param(3, 'phantom') === false, 'bool param');
  ok(world.strip(3).fader === 0, 'strip 3 fader falls back to 0 (no state yet)');
  ok(world.status(3, 'fader') === 'unsupported', 'minimal golden caps lacks fader (real engines list it)');
  ok(world.strip(1).muted === true, 'state mute lands');
  ok(world.mainFader === 0.800000011920929, 'main fader from state');
  ok(world.firstValueAt(3, 'gain') === 1000, 'first-value counter stamped');
}

// ---- pending / confirm / timeout / revert ----

{
  const { ctl, tr, world, runTimers } = setup();
  void runTimers;
  feedAttach(tr);
  tr.sent.length = 0;
  ok(world.setParam(3, 'gain', 20) === true, 'setParam accepted');
  ok(world.param(3, 'gain') === 20, 'optimistic display wins');
  ok(world.status(3, 'gain') === 'pending', 'status pending while unconfirmed');
  ok(tr.sent.length === 1, 'leading edge sends at once');
  ok((JSON.parse(tr.sent[0]!) as { cmd: string }).cmd === 'param', 'confirm cmd is param');
  // Confirm with the same value clears pending.
  tr.feedText(JSON.stringify({ type: 'param', ch: 3, field: 'gain', value: 20 }));
  ok(world.status(3, 'gain') === 'known' && world.param(3, 'gain') === 20, 'confirm clears pending');
  // New edit, no confirm: display reverts after the timeout.
  ok(world.setParam(5, 'gain', 30) === true, 'second edit accepted');
  ok(world.param(5, 'gain') === 30, 'optimistic shows');
  ctl.now += 1999;
  ok(world.sweep(ctl.now).length === 0, 'no expiry before timeout');
  ctl.now += 1;
  const expired = world.sweep(ctl.now);
  ok(expired.includes('5:gain'), 'pending expires at 2000 ms');
  ok(world.param(5, 'gain') === null, 'display reverts to unknown (no confirmed value)');
  ok(world.status(5, 'gain') === 'unknown', 'status back to unknown');
  // Coercion: wrong kind refused, floats clamp.
  ok(world.setParam(3, 'gain', 'loud' as never) === false, 'wrong kind refused');
  ok(world.setParam(3, 'phantom', 1 as never) === false, 'bool strict');
  ok(world.setParam(3, 'gain', 999) === true && world.param(3, 'gain') === 60, 'float clamps to caps max');
}

// ---- coalescing: leading edge + latest wins <= 60/s ----

{
  const { ctl, tr, world, runTimers } = setup();
  feedAttach(tr);
  tr.sent.length = 0;
  ok(world.setParam(0, 'gain', 10) === true, 'first write');
  ok(world.setParam(0, 'gain', 11) === true, 'second write coalesced');
  ok(world.setParam(0, 'gain', 12) === true, 'third write coalesced');
  ok(tr.sent.length === 1, 'one send on the leading edge');
  ok((JSON.parse(tr.sent[0]!) as { value: number }).value === 10, 'edge carries first value');
  ctl.now += 5;
  runTimers();
  ok(tr.sent.length === 2, 'flush sends once more');
  ok((JSON.parse(tr.sent[1]!) as { value: number }).value === 12, 'flush carries the latest value');
  ok(world.param(0, 'gain') === 12, 'display tracks latest');
}

// ---- fader/mute via dedicated cmds + state confirm ----

{
  const { tr, world } = setup();
  feedAttach(tr);
  // Golden caps.json is minimal (gain/phantom only): fader/mute need the
  // full rows (rich caps), as on a real engine.
  tr.feedText(richCaps());
  tr.sent.length = 0;
  ok(world.setFader(1, 0.9) === true, 'setFader accepted');
  ok(JSON.parse(tr.sent[0]!).cmd === 'fader', 'fader uses fader cmd');
  ok(world.strip(1).fader === 0.9, 'fader optimistic');
  ok(world.status(1, 'fader') === 'pending', 'fader pending');
  tr.feedText(JSON.stringify({ type: 'state', driver: 'sim', main_fader: null, strips: [{ ch: 1, name: 'Vox', fader: 0.9, muted: true }] }));
  ok(world.status(1, 'fader') === 'known', 'state frame confirms fader');
  ok(world.setMute(1, false) === true && JSON.parse(tr.sent[tr.sent.length - 1]!).cmd === 'mute', 'mute cmd');
  ok(world.setMainFader(0.5) === true && world.mainFader === 0.5, 'main pending');
}

// ---- focus: debounced, MRU first, no duplicates ----

{
  const { tr, world, runTimers } = setup();
  feedAttach(tr);
  tr.sent.length = 0;
  world.setVisible([5, 6, 7]);
  world.setOpenChannel(2);
  ok(tr.sent.length === 0, 'focus debounced');
  runTimers();
  ok(tr.sent.length === 1, 'one focus frame');
  const cmd = JSON.parse(tr.sent[0]!) as { cmd: string; chs: number[] };
  ok(cmd.cmd === 'focus' && JSON.stringify(cmd.chs) === JSON.stringify([2, 5, 6, 7]), 'MRU first: open channel, then visible');
  world.setVisible([5, 6, 7]);
  world.setOpenChannel(2);
  runTimers();
  ok(tr.sent.length === 1, 'identical focus not re-sent');
}

// ---- meters: binary + JSON decode into stable typed arrays ----

function metersBin(levels: number[]): ArrayBuffer {
  const out = new Uint8Array(12 + levels.length * 2);
  const v = new DataView(out.buffer);
  out[0] = 1;
  out[1] = 1;
  v.setUint16(2, levels.length, true);
  v.setUint32(4, 9, true);
  v.setUint32(8, 111, true);
  levels.forEach((l, i) => v.setUint16(12 + i * 2, Math.round(l * 65535), true));
  return out.buffer;
}
function grBin(ch: number, gate: number, comp: number): ArrayBuffer {
  const out = new Uint8Array(15);
  const v = new DataView(out.buffer);
  out[0] = 2;
  out[1] = 1;
  v.setUint16(2, 1, true);
  out[12] = ch;
  out[13] = Math.round(gate * 4);
  out[14] = Math.round(comp * 4);
  return out.buffer;
}

{
  const { tr, world } = setup();
  feedAttach(tr);
  const before = world.meters.level;
  tr.feedBinary(metersBin([0.5, 0.25, 0, 0.75]));
  ok(world.meters.level === before, 'no meter reallocation on binary frame');
  ok(Math.abs(world.meters.level[0]! - 0.5) < 1e-4, 'binary meters decoded');
  ok(world.meters.peak[0]! >= 0.5, 'peak tracks');
  tr.feedText(JSON.stringify({ type: 'gr', ch: 1, gate: 3.5, comp: 0 }));
  ok(world.meters.grGate[1] === 3.5 && world.meters.grComp[1] === 0, 'JSON gr decoded');
  tr.feedBinary(grBin(2, 4, 1));
  ok(world.meters.grGate[2] === 4 && world.meters.grComp[2] === 1, 'binary gr decoded');
  tr.feedText(JSON.stringify({ type: 'spectrum', ch: 0, bins: Array.from({ length: 64 }, (_, i) => i / 64) }));
  ok(world.meters.spectrum(0)?.length === 64, 'spectrum cached per channel');
  ok(world.meters.spectrum(9) === null, 'no spectrum for unseen channel');
  // Tolerant frames: unknown types + stats never break the world.
  const v0 = world.version;
  tr.feedText('{"type":"future_r2_thing","n":1}');
  tr.feedText(JSON.stringify({ type: 'stats', meters_sent: 1, meters_dropped: 0, gr_sent: 0, gr_dropped: 0, spectrum_sent: 0, spectrum_dropped: 0, queue_depth: 0, binary: true }));
  ok(world.version === v0 + 1, 'unknown frames bump once (stats stored, unknown ignored)', `${world.version} vs ${v0}`);
}

// ---- locks / denies / clients ----

{
  const { tr, world } = setup();
  feedAttach(tr);
  tr.feedText(gold('locks.json'));
  ok(world.strip(0).locked === true && world.lockedBy(0) === 'client-1', 'lock lands per strip');
  ok(world.strip(1).locked === false, 'unlocked strip clean');
  tr.feedText(gold('error.json'));
  ok(world.lastDeny?.lockedBy === 'client-1' && world.lastDeny?.ch === 0, 'deny recorded');
  world.lockStrip(4);
  ok((JSON.parse(tr.sent[tr.sent.length - 1]!) as { cmd: string }).cmd === 'lock', 'lock cmd');
  world.unlockStrip(4);
  ok((JSON.parse(tr.sent[tr.sent.length - 1]!) as { cmd: string }).cmd === 'unlock', 'unlock cmd');
}

// ---- channel views over the rich caps ----

{
  const { tr, world } = setup();
  tr.feedText(richCaps());
  tr.feedText(JSON.stringify({
    type: 'params', ch: 0, values: {
      gain: 30, trim: -2, phantom: false, polarity: true, delay_on: true, delay_ms: 25,
      pan: -50, hpf_on: true, hpf_freq: 90, hpf_slope: 2, eq_on: true,
      eq1_gain: 3, eq1_freq: 120, eq1_q: 0.7, eq1_type: 1,
      eq5_gain: -2, eq5_freq: 16000, eq5_q: 0.7, eq5_type: 4,
      gate_on: true, gate_thr: -40, gate_range: 50, gate_att: 2, gate_hold: 60, gate_rel: 200,
      comp_on: true, comp_thr: -18, comp_ratio: 4, comp_att: 12, comp_hold: 30, comp_rel: 150, comp_knee: 2, comp_makeup: 6,
      send1_level: -10, send1_tap: 0, send1_on: true, color: 4, name: 'Keys', source: 'Lcl 05',
    },
  }));
  const eq = world.channelEq(0);
  ok(eq.on.value === true && eq.on.status === 'known', 'eq on');
  ok(eq.bands[0]!.type === 'LS' && eq.bands[0]!.g.value === 3, 'band1 maps eq1_*');
  ok(eq.hpf.slope.value === 24, 'hpf slope 2 -> 24');
  ok(eq.band5 !== null && eq.band5.type === 'HS' && eq.band5.g.value === -2, 'eq5 exposed as band5');
  const dyn = world.channelDyn(0);
  ok(dyn.gate.thr.value === -40 && dyn.gate.range.value === 50, 'gate view');
  ok(dyn.comp.ratio.value === 4 && dyn.comp.makeup.value === 6, 'comp view');
  ok(world.param(0, 'comp_hold') === 30, 'comp_hold param-level (no design knob)');
  const sends = world.channelSends(0);
  ok(sends.length === 16, '16 sends');
  ok(sends[0]!.pre.value === true && sends[0]!.on.value === true, 'send1 pre/on');
  ok(sends[0]!.value.status === 'known' && Math.abs(sends[0]!.value.value! - 0.5) < 1e-9, 'send level via fader law (-10 dB -> 0.5)', String(sends[0]!.value.value));
  ok(sends[1]!.value.value === null && sends[1]!.value.status === 'unknown', 'unseen send unknown');
  const inp = world.channelInput(0);
  ok(inp.gain.value === 30 && inp.invert.value === true && inp.delayMs.value === 25, 'input view');
  const s = world.strip(0);
  ok(s.name === 'Keys' && s.source === 'Lcl 05', 'name/source from params');
  ok(s.colour === '#2563eb', 'colour from color param', s.colour);
  ok(s.pan === -0.5, 'pan /100', String(s.pan));
  // Write helpers round trip through the mapping.
  tr.sent.length = 0;
  ok(world.writePan(0, -0.5) === true, 'writePan');
  ok((JSON.parse(tr.sent[tr.sent.length - 1]!) as { value: number }).value === -50, 'pan writes -100..100');
  ok(world.writeSendLevel(0, 1, 1) === true, 'writeSendLevel');
  ok((JSON.parse(tr.sent[tr.sent.length - 1]!) as { value: number }).value === 10, 'full send -> +10 dB');
  ok(world.writeSendPre(0, 1, false) === true, 'writeSendPre');
  ok((JSON.parse(tr.sent[tr.sent.length - 1]!) as { value: number }).value === 1, 'post -> tap 1');
  ok(world.writeEqBandType(0, 1, 'HS') === true, 'writeEqBandType');
  ok((JSON.parse(tr.sent[tr.sent.length - 1]!) as { value: number }).value === 4, 'HS -> HShv');
  ok(world.writeSlope(0, 24) === true, 'writeSlope');
}

console.log(`engineWorld: ${passed} passed, ${failed} failed`);
if (failures.length) {
  for (const f of failures) console.log('FAIL', f);
  process.exit(1);
}

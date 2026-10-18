// W2a protocol tests: parsers against ALL golden frames + commands and the
// binary hex fixtures. Run: node --experimental-strip-types protocol.test.ts

import { readdirSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  cmdCaptureScene,
  cmdConnect,
  cmdDeleteScene,
  cmdFader,
  cmdFocus,
  cmdListShows,
  cmdLock,
  cmdMainFader,
  cmdMute,
  cmdParam,
  cmdRecallScene,
  cmdUnlock,
  parseBinary,
  parseEvent,
} from './protocol.ts';

const here = dirname(fileURLToPath(import.meta.url));
const GOLDEN = join(here, '..', '..', '..', '..', '..', 'crates', 'engine', 'tests', 'golden');
const GOLDEN_R2 = join(here, '..', '..', '..', '..', '..', 'crates', 'engine', 'tests', 'golden_r2');
const GOLDEN_BIN = join(here, '..', '..', '..', '..', '..', 'crates', 'engine', 'tests', 'golden_bin');
const SERVE_GOLDEN = join(here, '..', '..', '..', '..', '..', 'crates', 'serve', 'tests', 'golden');

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

function eq(a: unknown, b: unknown): boolean {
  if (typeof a !== typeof b) return false;
  if (a === null || b === null || typeof a !== 'object') return a === b;
  if (Array.isArray(a) || Array.isArray(b)) {
    if (!Array.isArray(a) || !Array.isArray(b) || a.length !== b.length) return false;
    return a.every((v, i) => eq(v, b[i]));
  }
  const ra = a as Record<string, unknown>;
  const rb = b as Record<string, unknown>;
  const ka = Object.keys(ra).sort();
  const kb = Object.keys(rb).sort();
  return ka.length === kb.length && ka.every((k, i) => k === kb[i] && eq(ra[k], rb[k]));
}

function approx(a: number, b: number, eps = 1e-4): boolean {
  return Math.abs(a - b) <= eps;
}

// ---- every golden frame parses to its kind ----

const frameFiles = readdirSync(GOLDEN).filter((f) => f.endsWith('.json') && !f.startsWith('cmd_'));
ok(frameFiles.length >= 13, 'golden frame count >= 13', String(frameFiles.length));

const byKind: Record<string, unknown> = {};
for (const f of frameFiles) {
  const text = readFileSync(join(GOLDEN, f), 'utf8');
  const evt = parseEvent(text);
  ok(evt !== null, `parses ${f}`);
  if (evt && evt.kind !== 'unknown') byKind[evt.kind] = JSON.parse(text);
}

// Spot checks against the golden values.
{
  const caps = byKind['caps'] as { params: unknown[]; topology: unknown[]; unsupported: string[]; meter_taps: string[] };
  ok(Array.isArray(caps?.params), 'caps params array');
  ok(Array.isArray(caps?.topology), 'caps topology array');
  ok(caps?.unsupported.includes('usbreturn'), 'caps unsupported has usbreturn');
  ok(caps?.meter_taps.includes('in'), 'caps meter_taps has in');
  const st = byKind['state'] as { strips: Array<{ ch: number; fader: number }> };
  ok(st?.strips.length === 2 && st.strips[0]!.ch === 0, 'state strips');
  const pr = byKind['param'] as { ch: number; field: string; value: unknown };
  ok(pr?.ch === 2 && pr?.field === 'gain', 'param ch/field');
  const ps = byKind['params'] as { ch: number; values: Record<string, unknown> };
  ok(ps?.ch === 3 && (ps?.values as Record<string, unknown>)['gain'] === 12.5, 'params values');
  const mt = byKind['meters'] as { levels: number[] };
  ok(eq(mt?.levels, [0.5, 0.25, 0, 0.75]), 'meters levels');
  const sp = byKind['spectrum'] as { bins: number[] };
  ok(sp?.bins.length === 64, 'spectrum 64 bins');
  const gr = byKind['gr'] as { ch: number; gate: number; comp: number };
  ok(gr?.ch === 1 && gr?.gate === 3.5, 'gr gate');
  const you = byKind['you'] as { id: string };
  ok(you?.id === 'client-1', 'you id');
  const locks = byKind['locks'] as { holds: Record<string, string> };
  ok(locks?.holds['0'] === 'client-1', 'locks holds');
  const err = byKind['error'] as { locked_by: string };
  ok(err?.locked_by === 'client-1', 'error locked_by');
  const rec = byKind['recalled'] as { overrode_locks: number[] };
  ok(eq(rec?.overrode_locks, [1]), 'recalled locks');
  const shows = byKind['shows'] as { shows: unknown[] };
  ok(shows?.shows.length === 1, 'shows library');
  const cl = byKind['clients'] as { count: number; writers: string[] };
  ok(cl?.count === 2, 'clients count');
}

// ---- R2 golden frames ----

const r2Files = readdirSync(GOLDEN_R2).filter((f) => f.endsWith('.json') && !f.startsWith('cmd_') && !f.endsWith('.bin.hex'));
for (const f of r2Files) {
  const text = readFileSync(join(GOLDEN_R2, f), 'utf8');
  const evt = parseEvent(text);
  ok(evt !== null && evt.kind !== 'unknown', `R2 parses ${f} as ${evt?.kind}`);
}
{
  const dev = parseEvent(readFileSync(join(GOLDEN_R2, 'devices.json'), 'utf8'));
  ok(dev?.kind === 'devices' && dev.list.length === 2 && dev.list[0]!.id === 'foh', 'devices list');
  const link = parseEvent(readFileSync(join(GOLDEN_R2, 'link.json'), 'utf8'));
  ok(link?.kind === 'link' && link.fs === 48000, 'link fs');
  const info = parseEvent(readFileSync(join(GOLDEN_R2, 'info.json'), 'utf8'));
  ok(info?.kind === 'info' && info.driver === 'sim', 'info driver');
  const taps = parseEvent(readFileSync(join(GOLDEN_R2, 'taps.json'), 'utf8'));
  ok(taps?.kind === 'taps' && taps.tap === 'in' && taps.levels.length === 4, 'taps frame');
  const ht = parseEvent(readFileSync(join(GOLDEN_R2, 'hello_taps.json'), 'utf8'));
  ok(ht?.kind === 'hello' && ht.taps === true, 'hello taps');
  const sc = parseEvent(readFileSync(join(GOLDEN_R2, 'scenes.json'), 'utf8'));
  ok(sc?.kind === 'scenes' && sc.scenes.length === 2, 'numbered scenes');
  const fx = parseEvent(readFileSync(join(GOLDEN_R2, 'fx.json'), 'utf8'));
  ok(fx?.kind === 'fx' && fx.slots[0]!.type === 'Hall', 'fx slots');
  const rt = parseEvent(readFileSync(join(GOLDEN_R2, 'routing.json'), 'utf8'));
  ok(rt?.kind === 'routing' && rt.patches[0]!.group === 'Net', 'routing patches');
  const od = parseEvent(readFileSync(join(GOLDEN_R2, 'offline_diff.json'), 'utf8'));
  ok(od?.kind === 'offline_diff' && od.count === 1, 'offline_diff');
}

// ---- serve stats golden ----

{
  const st = parseEvent(readFileSync(join(SERVE_GOLDEN, 'stats.json'), 'utf8'));
  ok(st?.kind === 'stats' && st.binary === true && st.meters_sent === 120, 'serve stats');
}

// ---- command builders vs golden cmd files ----

const builders: Record<string, () => string> = {
  cmd_fader: () => cmdFader(0, 0.5),
  cmd_main_fader: () => cmdMainFader(0.55),
  cmd_mute: () => cmdMute(0, true),
  cmd_param: () => cmdParam(1, 'gain', 12.5),
  cmd_focus: () => cmdFocus([5]),
  cmd_focus_multi: () => cmdFocus([3, 7]),
  cmd_connect: () => cmdConnect('sim'),
  cmd_list_shows: () => cmdListShows(),
  cmd_capture_scene: () => cmdCaptureScene('gig', 'verse'),
  cmd_recall_scene: () => cmdRecallScene('gig', 'verse', 0),
  cmd_delete_scene: () => cmdDeleteScene('gig', 'verse'),
  cmd_lock: () => cmdLock(0),
  cmd_unlock: () => cmdUnlock(0),
};
const cmdFiles = readdirSync(GOLDEN).filter((f) => f.startsWith('cmd_'));
for (const f of cmdFiles) {
  const name = f.replace(/\.json$/, '');
  const golden = JSON.parse(readFileSync(join(GOLDEN, f), 'utf8'));
  // Command files are NOT events.
  ok(parseEvent(JSON.stringify(golden)) === null, `${name} is not an event`);
  if (name === 'cmd_set_main_fader') {
    // Accepted alias of main_fader (Tauri command name); builders emit main_fader.
    ok(golden.cmd === 'set_main_fader' && golden.value === 0.55, 'set_main_fader alias shape');
    continue;
  }
  const build = builders[name];
  ok(!!build, `${name} has a builder`);
  if (build) ok(eq(JSON.parse(build()), golden), `${name} builder matches golden`);
}
// R2 golden cmds: known tags (builders exist for all; spot-check shapes).
{
  const known = new Set([
    'scan', 'connect_device', 'disconnect', 'info', 'scene_save', 'scene_recall',
    'scene_rename', 'scene_delete', 'scene_clear', 'fx_set_type', 'fx_param',
    'fx_bypass', 'fx_on', 'patch_set', 'patch_clear', 'offline', 'offline_push',
    'offline_discard', 'hello',
  ]);
  const r2cmds = readdirSync(GOLDEN_R2).filter((f) => f.startsWith('cmd_'));
  ok(r2cmds.length >= 10, 'R2 cmd count', String(r2cmds.length));
  for (const f of r2cmds) {
    const golden = JSON.parse(readFileSync(join(GOLDEN_R2, f), 'utf8'));
    ok(known.has(golden.cmd), `R2 cmd known: ${golden.cmd}`);
    ok(parseEvent(JSON.stringify(golden)) === null, `${f} is not an event`);
  }
}

// ---- binary fixtures ----

function hexFile(p: string): Uint8Array {
  return new Uint8Array(Buffer.from(readFileSync(p, 'utf8').trim(), 'hex'));
}
{
  const m = parseBinary(hexFile(join(GOLDEN_BIN, 'meters.bin.hex')));
  ok(m?.kind === 'meters' && m.levels.length === 4, 'bin meters header');
  if (m?.kind === 'meters') {
    ok(approx(m.levels[0]!, 0.5) && approx(m.levels[1]!, 0.25) && approx(m.levels[2]!, 0) && approx(m.levels[3]!, 0.75), 'bin meters levels match JSON golden');
    ok(m.seq === 7, 'bin meters seq');
  }
  const g = parseBinary(hexFile(join(GOLDEN_BIN, 'gr.bin.hex')));
  ok(g?.kind === 'gr' && g.ch === 1 && approx(g.gate, 3.5) && approx(g.comp, 0), 'bin gr matches JSON golden');
  const ti = parseBinary(hexFile(join(GOLDEN_R2, 'taps_in.bin.hex')));
  ok(ti?.kind === 'taps' && ti.tap === 'in' && ti.levels.length > 0, 'bin taps_in');
  const tg = parseBinary(hexFile(join(GOLDEN_R2, 'taps_gr.bin.hex')));
  ok(tg?.kind === 'taps' && (tg.tap === 'gr_gate' || tg.tap === 'gr_comp'), 'bin taps_gr id maps', tg?.kind === 'taps' ? tg.tap : '?');
}

// ---- never throws ----

for (const bad of ['', 'not json', 'null', '42', '[]', '{}', '{"cmd":"fader"}', '{"type":42}', '{"type":"state"}', '{"type":"state","strips":"x"}']) {
  let threw = false;
  try {
    parseEvent(bad);
  } catch {
    threw = true;
  }
  ok(!threw, `parseEvent never throws: ${JSON.stringify(bad).slice(0, 40)}`);
}
{
  const fut = parseEvent('{"type":"future_frame_v9","x":[1,2]}');
  ok(fut?.kind === 'unknown' && fut.eventType === 'future_frame_v9', 'unknown type is lenient');
  const ok2 = parseEvent('{"type":"caps","params":[]}');
  ok(ok2?.kind === 'caps', 'minimal caps parses (R2-less)');
}
for (const bad of [new Uint8Array(0), new Uint8Array(11), new Uint8Array([9, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]), new Uint8Array([1, 9, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0])]) {
  let threw = false;
  try {
    parseBinary(bad);
  } catch {
    threw = true;
  }
  ok(!threw && parseBinary(bad) === null, 'parseBinary rejects bad input');
}

console.log(`protocol: ${passed} passed, ${failed} failed`);
if (failures.length) {
  for (const f of failures) console.log('FAIL', f);
  process.exit(1);
}

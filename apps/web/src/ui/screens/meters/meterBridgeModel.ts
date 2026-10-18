// Meter-bridge group model (X-Air Edit Meters pattern).
//
// Pure, rune-free, DOM-free: given the live EngineWorld (or null offline)
// it builds the grouped ladder list the overlay renders. The group order
// follows the owner-approved X-Air Edit pattern: Analog Inputs 1-16 +
// 17/18, Effects Sends 1-4, FX Returns, Bus Outputs 1-6, Mon/Solo,
// Main L/R, USB Returns, P16 sends.
//
// Honesty rules (PLAN W2 + backlog item):
// - Meter levels come ONLY from the live engine path (world.meters.level,
//   negotiated over WS). Offline every ladder reads null (dark, -inf).
// - On xair only kind 'in' strips have a rig-verified blob index
//   (proto-xair: blob 0..15 = channels; families past the channels are
//   unverified, so the blob landing in those slots is NOT their level).
//   Non-input strips on xair therefore read null too — same guard as
//   live.liveLevelDb, duplicated here (engineBridge is another lane).
// - Groups the desk/engine has no strips for collapse with a reason;
//   they never render phantom ladders.

import { linearToDb } from '../../state/live.ts';
import type { EngineWorld } from '../../world/engineWorld/engineWorld.ts';
import type { StripKind } from '../../world/types.ts';

/** One ladder: a global strip slot on the live world. */
export interface BridgeLadder {
  /** Global strip index (world.meters slot). */
  ch: number;
  /** Short scribble (live strip name when online, null offline). */
  label: string | null;
  /** Sub label (family-relative number, e.g. `3`). */
  sub: string;
}

/** One pattern group: ladders when the desk has the strips, else collapsed. */
export interface BridgeGroup {
  key: string;
  title: string;
  ladders: BridgeLadder[];
  /** Set when collapsed: why there are no ladders (desk name included). */
  collapsedReason: string | null;
}

/** Meter scale floor/ceiling in dB (X-Air Edit pattern: 0..-50). */
export const BRIDGE_TOP_DB = 0;
export const BRIDGE_FLOOR_DB = -50;

/** dB thresholds for the ladder segment colours (top -> bottom). */
export function segColor(db: number): 'red' | 'amber' | 'green' {
  if (db >= -6) return 'red';
  if (db >= -12) return 'amber';
  return 'green';
}

/** Collect global strip indices of one family kind, in strip order. */
function stripsOf(world: EngineWorld, kind: StripKind): number[] {
  const out: number[] = [];
  for (let ch = 0; ch < world.stripCount; ch++) {
    if (world.kindOf(ch) === kind) out.push(ch);
  }
  return out;
}

/** Live level in dB for a strip, or null when there is honestly no data:
 * offline, or an xair strip past the verified channel blob region. */
export function liveLevelDb(world: EngineWorld | null, online: boolean, ch: number): number | null {
  if (!online || !world || !world.capsReady) return null;
  if (ch < 0 || ch >= world.meters.size) return null;
  if (world.driver === 'xair' && world.kindOf(ch) !== 'in') return null;
  return linearToDb(world.meters.level[ch] ?? 0);
}

/** Live peak-hold level in dB (same honesty guard as the level). */
export function livePeakDb(world: EngineWorld | null, online: boolean, ch: number): number | null {
  const lv = liveLevelDb(world, online, ch);
  if (lv === null || !world) return null;
  return linearToDb(world.meters.peak[ch] ?? 0);
}

/** True while the strip's clip latch is lit (same honesty guard). */
export function liveClipOn(world: EngineWorld | null, online: boolean, ch: number, nowMs: number): boolean {
  if (liveLevelDb(world, online, ch) === null || !world) return false;
  return (world.meters.clipUntil[ch] ?? 0) > nowMs;
}

function laddersFor(world: EngineWorld, online: boolean, chs: number[]): BridgeLadder[] {
  return chs.map((ch, i) => {
    let label: string | null = null;
    if (online) {
      try {
        label = world.strip(ch).name || null;
      } catch {
        label = null;
      }
    }
    return { ch, label, sub: String(i + 1) };
  });
}

/**
 * Build the pattern groups. `model` names the desk in collapse reasons
 * (null offline). Offline (world null or !online): the pattern skeleton
 * with empty ladders, so the overlay shows honest dark chrome, never data.
 */
export function buildMeterGroups(
  world: EngineWorld | null,
  online: boolean,
  model: string | null,
): BridgeGroup[] {
  const desk = model ?? 'this desk';
  if (!online || !world || !world.capsReady) {
    const skel = (key: string, title: string): BridgeGroup => ({ key, title, ladders: [], collapsedReason: 'offline' });
    return [
      skel('inputs', 'Analog Inputs 1–16'),
      skel('inputs17', 'Analog Inputs 17–18'),
      skel('fxsends', 'Effects Sends 1–4'),
      skel('fxrets', 'FX Returns 1–4'),
      skel('buses', 'Bus Outputs 1–6'),
      skel('monsolo', 'Mon/Solo'),
      skel('main', 'Main L/R'),
      skel('usb', 'USB Returns 1–2'),
      skel('p16', 'P16 Sends 1–16'),
    ];
  }
  const groups: BridgeGroup[] = [];
  const ins = stripsOf(world, 'in');
  groups.push({
    key: 'inputs',
    title: 'Analog Inputs 1–16',
    ladders: laddersFor(world, true, ins.slice(0, 16)),
    collapsedReason: ins.length ? null : `no input strips on ${desk}`,
  });
  const rest = ins.slice(16);
  groups.push({
    key: 'inputs17',
    title: rest.length > 2 ? `Analog Inputs 17–${16 + rest.length}` : 'Analog Inputs 17–18',
    ladders: laddersFor(world, true, rest),
    // Aux 17/18 has no strips on any known driver; extra 'in' strips
    // (sim Ch 17+) show as ladders instead.
    collapsedReason: rest.length ? null : `no Aux 17/18 strips on ${desk}`,
  });
  const snd = stripsOf(world, 'fxsnd');
  groups.push({
    key: 'fxsends',
    title: snd.length ? `Effects Sends 1–${snd.length}` : 'Effects Sends 1–4',
    ladders: laddersFor(world, true, snd),
    collapsedReason: snd.length ? null : `no FX-send strips on ${desk}`,
  });
  const ret = stripsOf(world, 'ret');
  groups.push({
    key: 'fxrets',
    title: ret.length ? `FX Returns 1–${ret.length}` : 'FX Returns 1–4',
    ladders: laddersFor(world, true, ret),
    collapsedReason: ret.length ? null : `no FX-return strips on ${desk}`,
  });
  const bus = stripsOf(world, 'bus');
  groups.push({
    key: 'buses',
    title: bus.length ? `Bus Outputs 1–${bus.length}` : 'Bus Outputs 1–6',
    ladders: laddersFor(world, true, bus),
    collapsedReason: bus.length ? null : `no bus strips on ${desk}`,
  });
  groups.push({ key: 'monsolo', title: 'Mon/Solo', ladders: [], collapsedReason: `monitor bus is not a metered strip on ${desk}` });
  const main = stripsOf(world, 'main');
  groups.push({
    key: 'main',
    title: 'Main L/R',
    // One stereo pair per main strip (XR18/sim: one strip -> L/R ladders).
    ladders: main.flatMap((ch) => [
      { ch, label: onlineLabel(world, ch, 'L'), sub: 'L' },
      { ch, label: onlineLabel(world, ch, 'R'), sub: 'R' },
    ]),
    collapsedReason: main.length ? null : `no main strip on ${desk}`,
  });
  groups.push({ key: 'usb', title: 'USB Returns 1–2', ladders: [], collapsedReason: `no USB-return strips on ${desk}` });
  groups.push({ key: 'p16', title: 'P16 Sends 1–16', ladders: [], collapsedReason: `no P16 strips on ${desk}` });
  return groups;
}

function onlineLabel(world: EngineWorld, ch: number, sub: string): string | null {
  try {
    const n = world.strip(ch).name;
    return n ? `${n} ${sub}` : sub;
  } catch {
    return sub;
  }
}

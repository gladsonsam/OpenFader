// Write-through bridge: fixture channel edits -> live EngineWorld commands.
//
// The fixture store (uiStore) owns the design display model; when a live
// relay link is up (`live.phase === 'online'`), every desk-mapped edit is
// ALSO forwarded to the EngineWorld so fader/mute/pan/sends (and EQ, dyn,
// input, patch) moves reach the desk. Fixture mode is untouched: with no
// live world every bridge call is a silent no-op returning false.
//
// Mapping is topology-driven (never the sim-only index map): fixture ids
// resolve to the Nth strip of their family kind on the LIVE driver, so the
// same UI drives sim (77 strips), XR18 (35) and DL32S honestly — families
// the desk lacks (grp subgroups as strips, Main C, sends past the desk's)
// resolve null and stay fixture-local.
//
// Value units: fader/mute/bools ride directly; pan converts design -1..1
// to engine -100..100; sends ride the existing design01<->dB law helpers;
// EQ/dyn/input ride engineering units (dB/Hz/ms) with caps-driven
// coercion inside setParam (out-of-range desk values clamp, unknown ids
// refuse). Not bridged (no desk equivalent or no id mapping): per-band EQ
// bypass (engine has a global switch only), FX type/params (engine param
// ids need the FX type tables the world does not retain), fixture scenes
// (a different library from numbered scenes), main mute (no engine cmd).
//
// Run: node --experimental-strip-types apps/web/src/ui/state/engineBridge.test.ts

import { lawForDriver } from '../world/mapping/faderLaw.ts';
import {
  COLOR_HEX,
  design01ToSendDb,
  designEqTypeToEngine,
  designPanToEngine,
  designSlopeToEngine,
  preToTap,
} from '../world/mapping/params.ts';
import type { StripKind, World } from '../world/types.ts';

/** Fixture id -> (family kind, 0-based ordinal). Null = no strip form. */
export function bridgeTarget(id: string): { kind: StripKind; index: number } | null {
  let m = /^in(\d+)$/.exec(id);
  if (m) {
    const n = parseInt(m[1]!, 10);
    return n >= 1 ? { kind: 'in', index: n - 1 } : null;
  }
  m = /^mix(\d+)$/.exec(id);
  if (m) {
    const n = parseInt(m[1]!, 10);
    return n >= 1 ? { kind: 'bus', index: n - 1 } : null;
  }
  m = /^fxs(\d+)$/.exec(id);
  if (m) {
    const n = parseInt(m[1]!, 10);
    return n >= 1 ? { kind: 'fxsnd', index: n - 1 } : null;
  }
  m = /^mtx(\d+)$/.exec(id);
  if (m) {
    const n = parseInt(m[1]!, 10);
    return n >= 1 ? { kind: 'mtx', index: n - 1 } : null;
  }
  m = /^dca(\d+)$/.exec(id);
  if (m) {
    const n = parseInt(m[1]!, 10);
    return n >= 1 ? { kind: 'dca', index: n - 1 } : null;
  }
  m = /^fx(\d+)$/.exec(id);
  if (m) {
    const n = parseInt(m[1]!, 10);
    return n >= 1 ? { kind: 'ret', index: n - 1 } : null;
  }
  if (id === 'mainlr') return { kind: 'main', index: 0 };
  // grpN (subgroups as strips), mainc, and anything else have no strip
  // equivalent on any desk family.
  return null;
}

/** Nth strip of a family kind on the live world; null when absent. */
export function bridgeStrip(world: World, id: string): number | null {
  const t = bridgeTarget(id);
  if (!t) return null;
  let seen = 0;
  for (let ch = 0; ch < world.stripCount; ch++) {
    if (world.kindOf(ch) !== t.kind) continue;
    if (seen === t.index) return ch;
    seen++;
  }
  return null;
}

/** Fader write-through (`mainlr` rides the LR main fader). */
export function bridgeFader(world: World, id: string, v01: number): boolean {
  if (id === 'mainlr') return world.setMainFader(v01);
  const ch = bridgeStrip(world, id);
  return ch === null ? false : world.setFader(ch, v01);
}

/** Mute write-through (main has no engine mute — fixture-only). */
export function bridgeMute(world: World, id: string, muted: boolean): boolean {
  if (id === 'mainlr') return false;
  const ch = bridgeStrip(world, id);
  return ch === null ? false : world.setMute(ch, muted);
}

/** Pan write-through (design -1..1 -> engine -100..100). */
export function bridgePan(world: World, id: string, p: number): boolean {
  const ch = bridgeStrip(world, id);
  if (ch === null || !Number.isFinite(p)) return false;
  return world.setParam(ch, 'pan', designPanToEngine(p));
}

/** Send write-through (fixture 0-based k -> engine send k+1, law inside). */
export function bridgeSend(world: World, id: string, k: number, v01: number): boolean {
  const ch = bridgeStrip(world, id);
  if (ch === null || !Number.isInteger(k) || k < 0 || !Number.isFinite(v01)) return false;
  return world.setParam(ch, `send${k + 1}_level`, design01ToSendDb(v01, lawForDriver(world.driver)));
}

/** Send pre/post write-through. */
export function bridgeSendPre(world: World, id: string, k: number, pre: boolean): boolean {
  const ch = bridgeStrip(world, id);
  if (ch === null || !Number.isInteger(k) || k < 0 || typeof pre !== 'boolean') return false;
  return world.setParam(ch, `send${k + 1}_tap`, preToTap(pre));
}

export type EqBridgeParam = 'f' | 'g' | 'q';

/** EQ write-through (band 0 = HPF freq; 1..4 = eqN_{freq,gain,q}). */
export function bridgeEq(world: World, id: string, band: number, param: EqBridgeParam, value: number): boolean {
  const ch = bridgeStrip(world, id);
  if (ch === null || !Number.isFinite(value)) return false;
  if (band === 0) {
    if (param !== 'f') return false;
    return world.setParam(ch, 'hpf_freq', value);
  }
  if (!Number.isInteger(band) || band < 1 || band > 4) return false;
  const leaf = param === 'f' ? 'freq' : param === 'g' ? 'gain' : 'q';
  return world.setParam(ch, `eq${band}_${leaf}`, value);
}

/** EQ band type write-through (LS/PEQ/HS; bands 1..4). */
export function bridgeEqType(world: World, id: string, band: number, t: 'LS' | 'PEQ' | 'HS'): boolean {
  const ch = bridgeStrip(world, id);
  if (ch === null || !Number.isInteger(band) || band < 1 || band > 4) return false;
  return world.setParam(ch, `eq${band}_type`, designEqTypeToEngine(t));
}

/** EQ on/off write-through (global switch; per-band bypass is fixture-only). */
export function bridgeEqOn(world: World, id: string, on: boolean): boolean {
  const ch = bridgeStrip(world, id);
  if (ch === null || typeof on !== 'boolean') return false;
  return world.setParam(ch, 'eq_on', on);
}

/** HPF on/write-through. */
export function bridgeHpfOn(world: World, id: string, on: boolean): boolean {
  const ch = bridgeStrip(world, id);
  if (ch === null || typeof on !== 'boolean') return false;
  return world.setParam(ch, 'hpf_on', on);
}

/** HPF slope write-through. */
export function bridgeHpfSlope(world: World, id: string, slope: 12 | 24): boolean {
  const ch = bridgeStrip(world, id);
  if (ch === null || (slope !== 12 && slope !== 24)) return false;
  return world.setParam(ch, 'hpf_slope', designSlopeToEngine(slope));
}

const DYN_PROPS: Record<string, readonly string[]> = {
  gate: ['thr', 'range', 'att', 'hold', 'rel'],
  comp: ['thr', 'ratio', 'att', 'rel', 'knee', 'makeup'],
};

/** Dynamics value write-through (`gate_thr`, `comp_ratio`, …). */
export function bridgeDyn(world: World, id: string, which: 'gate' | 'comp', prop: string, value: number): boolean {
  const ch = bridgeStrip(world, id);
  if (ch === null || !Number.isFinite(value)) return false;
  if (!(DYN_PROPS[which] ?? []).includes(prop)) return false;
  return world.setParam(ch, `${which}_${prop}`, value);
}

/** Gate/comp on write-through. */
export function bridgeDynOn(world: World, id: string, which: 'gate' | 'comp', on: boolean): boolean {
  const ch = bridgeStrip(world, id);
  if (ch === null || typeof on !== 'boolean') return false;
  return world.setParam(ch, `${which}_on`, on);
}

/** Input gain/trim write-through (dB, caps-clamped on the desk side). */
export function bridgeGainTrim(world: World, id: string, field: 'gain' | 'trim', value: number): boolean {
  const ch = bridgeStrip(world, id);
  if (ch === null || !Number.isFinite(value)) return false;
  return world.setParam(ch, field, value);
}

/** Phantom / polarity / delay-on write-through. */
export function bridgeInputBool(
  world: World,
  id: string,
  field: 'phantom' | 'polarity' | 'delay_on',
  value: boolean,
): boolean {
  const ch = bridgeStrip(world, id);
  if (ch === null || typeof value !== 'boolean') return false;
  return world.setParam(ch, field, value);
}

/** Delay time write-through (ms). */
export function bridgeDelayMs(world: World, id: string, value: number): boolean {
  const ch = bridgeStrip(world, id);
  if (ch === null || !Number.isFinite(value)) return false;
  return world.setParam(ch, 'delay_ms', value);
}

/** Rename write-through (blank names never reach here; store guards). */
export function bridgeName(world: World, id: string, name: string): boolean {
  const ch = bridgeStrip(world, id);
  if (ch === null || typeof name !== 'string' || !name) return false;
  return world.setParam(ch, 'name', name);
}

/** Recolour write-through (fixture hex -> engine color enum index). */
export function bridgeColor(world: World, id: string, colorHex: string): boolean {
  const ch = bridgeStrip(world, id);
  if (ch === null) return false;
  const idx = COLOR_HEX.findIndex(c => c.toLowerCase() === colorHex.toLowerCase());
  if (idx < 0) return false;
  return world.setParam(ch, 'color', idx);
}

/** Parse a fixture patch label (`Lcl 01`) into engine (group, 1-based index). */
export function bridgePatchLabel(label: string): { group: string; index: number } | null {
  const m = /^(Lcl|Net|USB)\s+(\d{1,2})$/.exec(label.trim());
  if (!m) return null;
  const index = parseInt(m[2]!, 10);
  if (index < 1 || index > 32) return null;
  return { group: m[1]!, index };
}

/** Patch write-through (fixture label -> engine group/index). */
export function bridgePatch(world: World, id: string, label: string): boolean {
  const ch = bridgeStrip(world, id);
  const p = bridgePatchLabel(label);
  if (ch === null || !p) return false;
  if (!world.routingSupported) return false;
  world.patchSet(ch, p.group, p.index);
  return true;
}

/** Unpatch write-through. */
export function bridgeUnpatch(world: World, id: string): boolean {
  const ch = bridgeStrip(world, id);
  if (ch === null || !world.routingSupported) return false;
  world.patchClear(ch);
  return true;
}

/** Mute-group membership write-through (desk-only; no fixture counterpart). */
export function bridgeMuteGroupAssign(world: World, id: string, group: number, member: boolean): boolean {
  const ch = bridgeStrip(world, id);
  if (ch === null || !Number.isInteger(group) || group < 0 || typeof member !== 'boolean') return false;
  if (world.groupCaps.muteGroups <= group) return false;
  world.muteGroupAssign(ch, group, member);
  return true;
}

/** Subgroup membership write-through (desk-only; no fixture counterpart). */
export function bridgeSubgroupAssign(world: World, id: string, group: number, member: boolean): boolean {
  const ch = bridgeStrip(world, id);
  if (ch === null || !Number.isInteger(group) || group < 0 || typeof member !== 'boolean') return false;
  if (world.groupCaps.subgroups <= group) return false;
  world.subgroupAssign(ch, group, member);
  return true;
}

/** Mute-group master mute write-through. */
export function bridgeMuteGroupMute(world: World, group: number, muted: boolean): boolean {
  if (!Number.isInteger(group) || group < 0 || typeof muted !== 'boolean') return false;
  if (world.groupCaps.muteGroups <= group) return false;
  world.muteGroup(group, muted);
  return true;
}

/** Subgroup master mute write-through. */
export function bridgeSubgroupMute(world: World, group: number, muted: boolean): boolean {
  if (!Number.isInteger(group) || group < 0 || typeof muted !== 'boolean') return false;
  if (world.groupCaps.subgroups <= group) return false;
  world.subgroupMute(group, muted);
  return true;
}

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
// (a different library from numbered scenes), Main LR solo (no verified
// desk leaf — the master solo control stays hidden).
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

/** Fixture id -> (family kind, 0-based ordinal). Null = no strip form.
 * `aux1` is the XR18 stereo aux input (`/rtn/aux`, its own one-strip `ret`
 * family in the xair topology): the index is the *family* ordinal (0 = the
 * aux strip itself). `bridgeStrip` resolves it family-aware below, never by
 * coarse `ret` ordinal (which would collide with the FX returns). */
export function bridgeTarget(id: string): { kind: StripKind; index: number } | null {
  if (id === 'aux1') return { kind: 'ret', index: 0 };
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

/** Nth strip of a family kind on the live world; null when absent.
 * `ret` ids resolve family-aware: `aux1` is the strip of the `aux` family
 * (the XR18 stereo aux input), `fxN` the Nth strip of the `fxret` family
 * (both are kind `ret`, so a coarse ordinal would collide). Other kinds
 * keep the historical kind-ordinal mapping. */
export function bridgeStrip(world: World, id: string): number | null {
  if (id === 'aux1') {
    const b = world.banks.find((bk) => bk.families.includes('aux'));
    const s = b?.strips[0];
    return typeof s === 'number' ? s : null;
  }
  const fxm = /^fx(\d+)$/.exec(id);
  if (fxm) {
    const n = parseInt(fxm[1]!, 10);
    if (!Number.isInteger(n) || n < 1) return null;
    const b = world.banks.find((bk) => bk.families.includes('fxret'));
    if (b) return n - 1 < b.strips.length ? (b.strips[n - 1] as number) : null;
    // No `fxret` family on this desk (older topology): fall through to
    // the coarse kind-ordinal mapping below.
  }
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

/** Mute write-through (main rides the Main LR master mute). */
export function bridgeMute(world: World, id: string, muted: boolean): boolean {
  if (typeof muted !== 'boolean') return false;
  if (id === 'mainlr') {
    world.setMainMute(muted);
    return true;
  }
  const ch = bridgeStrip(world, id);
  return ch === null ? false : world.setMute(ch, muted);
}

/** Solo write-through (monitor bus only; main LR has no desk leaf). */
export function bridgeSolo(world: World, id: string, soloed: boolean): boolean {
  if (typeof soloed !== 'boolean' || id === 'mainlr') return false;
  const ch = bridgeStrip(world, id);
  if (ch === null) return false;
  world.setSolo(ch, soloed);
  return true;
}

/** Pan write-through (design -1..1 -> engine -100..100). */
export function bridgePan(world: World, id: string, p: number): boolean {
  const ch = bridgeStrip(world, id);
  if (ch === null || !Number.isFinite(p)) return false;
  const field = world.supports('balance', ch) ? 'balance' : 'pan';
  return world.setParam(ch, field, designPanToEngine(p));
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

/** Send group-on write-through (desk send-mute leaf; fixture has no counterpart). */
export function bridgeSendGrpon(world: World, id: string, k: number, grouped: boolean): boolean {
  const ch = bridgeStrip(world, id);
  if (ch === null || !Number.isInteger(k) || k < 0 || typeof grouped !== 'boolean') return false;
  return world.setParam(ch, `send${k + 1}_grpon`, grouped);
}

/** Send tap-point write-through (desk `sendN_tap` enum 0..5; fixture has
 * no counterpart — the echo re-renders the row). Whole numbers only. */
export function bridgeSendTap(world: World, id: string, k: number, tap: number): boolean {
  const ch = bridgeStrip(world, id);
  if (ch === null || !Number.isInteger(k) || k < 0) return false;
  if (!Number.isInteger(tap) || tap < 0 || tap > 5) return false;
  return world.setParam(ch, `send${k + 1}_tap`, tap);
}

/** LR assign write-through (`{root}/mix/lr`; fixture has no counterpart). */
export function bridgeLrAssign(world: World, id: string, assigned: boolean): boolean {
  const ch = bridgeStrip(world, id);
  if (ch === null || typeof assigned !== 'boolean') return false;
  return world.setParam(ch, 'lr_assign', assigned);
}

/** Automix weight write-through (0..1; fixture has no counterpart). */
export function bridgeAutomixWeight(world: World, id: string, weight: number): boolean {
  const ch = bridgeStrip(world, id);
  if (ch === null || !Number.isFinite(weight)) return false;
  return world.setParam(ch, 'automix_weight', Math.min(1, Math.max(0, weight)));
}

/** Channel-insert bypass write-through (`{root}/insert/on`; fixture has
 * no counterpart). Unknown/unmapped strips and lock-checked callers stay
 * silent (false). */
export function bridgeInsertOn(world: World, id: string, on: boolean): boolean {
  const ch = bridgeStrip(world, id);
  if (ch === null || typeof on !== 'boolean') return false;
  return world.setParam(ch, 'insert_on', on);
}

/** Channel-insert select write-through (`{root}/insert/sel`, raw int
 * `0..=8` — the desk values are NOT 1..4 FX slots, so no slot mapping is
 * asserted; fixture has no counterpart). Whole-number clamped. */
export function bridgeInsertSel(world: World, id: string, sel: number): boolean {
  const ch = bridgeStrip(world, id);
  if (ch === null || !Number.isFinite(sel)) return false;
  return world.setParam(ch, 'insert_sel', Math.min(8, Math.max(0, Math.round(sel))));
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

/** FX rack write-through (slot 0-based; engine slots are 1-based). */
export function bridgeFxType(world: World, slot: number, type: string): boolean {
  if (!Number.isInteger(slot) || slot < 0 || typeof type !== 'string' || !type) return false;
  world.fxSetType(slot + 1, type);
  return true;
}

/** FX slot on write-through. */
export function bridgeFxOn(world: World, slot: number, on: boolean): boolean {
  if (!Number.isInteger(slot) || slot < 0 || typeof on !== 'boolean') return false;
  world.fxOn(slot + 1, on);
  return true;
}

/** FX slot bypass write-through. */
export function bridgeFxBypass(world: World, slot: number, bypass: boolean): boolean {
  if (!Number.isInteger(slot) || slot < 0 || typeof bypass !== 'boolean') return false;
  world.fxBypass(slot + 1, bypass);
  return true;
}

/** FX param write-through by knob index (resolves the param id from the
 * live type table, so fixture and desk knob orders can never drift). */
export function bridgeFxParamByIndex(world: World, slot: number, p: number, n: number): boolean {
  if (!Number.isInteger(slot) || slot < 0 || !Number.isInteger(p) || p < 0) return false;
  if (typeof n !== 'number' || !Number.isFinite(n)) return false;
  const live = world.fxSlots.find(s => s.slot === slot + 1);
  if (!live) return false;
  const def = world.fxCaps?.types.find(t => t.id === live.type || t.label === live.type);
  const id = def?.params[p]?.id;
  if (!id) return false;
  world.fxParam(slot + 1, id, Math.min(1, Math.max(0, n)));
  return true;
}

/** Subgroup master mute write-through. */
export function bridgeSubgroupMute(world: World, group: number, muted: boolean): boolean {
  if (!Number.isInteger(group) || group < 0 || typeof muted !== 'boolean') return false;
  if (world.groupCaps.subgroups <= group) return false;
  world.subgroupMute(group, muted);
  return true;
}

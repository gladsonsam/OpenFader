// uiLogic/inputDyn: input tab + phantom arm + dynamics.
// Pure, rune-free logic split out of state/uiLogic.ts (one version of
// the logic lives here; uiLogic.ts re-exports it). DOM-free, node-testable.

import { clamp, fromN, spec, type Channel } from '../../fixtures/designData.ts';
import { type World, withChannel, withState } from './types.ts';

// ---------- input tab (lock-checked toggles; spec drag math) ----------

/** Port of the 'g' branch of move(): normalised spec drag -> value. */
export function specFromDrag(specKey: string, startN: number, dyPx: number, fine: boolean): number {
  const s = spec(specKey);
  return fromN(s, clamp(startN + dyPx / 180 * (fine ? 0.25 : 1), 0, 1));
}

export function setSpecValue(w: World, id: string, specKey: string, apply: (c: Channel, v: number) => void, n: number): World {
  void specKey;
  return withChannel(w, id, c => { apply(c, fromN(spec(specKey), clamp(n, 0, 1))); });
}

export function toggleDelayOn(w: World, id: string): World {
  if (w.state.locked) return w;
  return withChannel(w, id, c => { if (c.input) c.input.delayOn = !c.input.delayOn; });
}

export function toggleInvert(w: World, id: string): World {
  if (w.state.locked) return w;
  return withChannel(w, id, c => { if (c.input) c.input.invert = !c.input.invert; });
}

/**
 * Two-tap 48V arm flow (port of togglePhantom without the timer): first tap
 * arms, second tap commits. Expiry is via expirePhantom, driven by the store's
 * 3 s timer exactly like the design's armTimer.
 */
export function togglePhantom(w: World, id: string): World {
  if (w.state.locked) return w;
  if (w.state.phantomArm === id) {
    const armed = withChannel(w, id, c => { if (c.input) c.input.phantom = !c.input.phantom; });
    return withState(armed, { phantomArm: null });
  }
  return withState(w, { phantomArm: id });
}

export function expirePhantom(w: World, id: string): World {
  if (w.state.phantomArm !== id) return w;
  return withState(w, { phantomArm: null });
}

export function phantomLabel(w: World, ch: Channel): string {
  const armed = w.state.phantomArm === ch.id;
  const on = ch.input?.phantom ?? false;
  if (armed) return on ? 'Tap again: 48V off' : 'Tap again: 48V on';
  return on ? '48V On' : '48V Off';
}
// ---------- dynamics (toggle lock-checked; values via spec math) ----------

export function setDynParam(
  w: World, id: string, which: 'gate' | 'comp', prop: string, n: number,
): World {
  const key = dynSpecKey(which, prop);
  return withChannel(w, id, c => {
    const d = c.dyn?.[which] as unknown as Record<string, number> | undefined;
    if (d) d[prop] = fromN(spec(key), clamp(n, 0, 1));
  });
}

export function dynSpecKey(which: 'gate' | 'comp', prop: string): string {
  const table: Record<string, string> = {
    'gate.thr': 'gthr', 'gate.range': 'range', 'gate.att': 'gatt', 'gate.hold': 'hold', 'gate.rel': 'grel',
    'comp.thr': 'cthr', 'comp.ratio': 'ratio', 'comp.knee': 'knee', 'comp.att': 'catt', 'comp.rel': 'crel', 'comp.makeup': 'makeup',
  };
  const key = table[which + '.' + prop];
  if (!key) throw new Error('unknown dyn param: ' + which + '.' + prop);
  return key;
}

export function toggleDyn(w: World, id: string, which: 'gate' | 'comp'): World {
  if (w.state.locked) return w;
  return withChannel(w, id, c => {
    const d = c.dyn?.[which];
    if (d) d.on = !d.on;
  });
}

/**
 * Reset defaults (design K() tables, mirrored by DYN_KNOBS defaultValue in
 * screens/channel/dyn/dynModel.ts — kept local so state/logic never imports
 * from screens). Restoring values also switches the section in so the reset
 * is audible. Lock-checked like toggleDyn.
 */
const DYN_RESET: Record<'gate' | 'comp', Record<string, number>> = {
  gate: { thr: -50, range: 40, att: 0.5, hold: 50, rel: 150 },
  comp: { thr: -20, ratio: 3, knee: 3, att: 10, rel: 120, makeup: 0 },
};

/** Default table a reset writes (copy; the store forwards these to the desk). */
export function dynResetDefaults(which: 'gate' | 'comp'): Record<string, number> {
  return { ...(DYN_RESET[which] ?? {}) };
}

export function resetDyn(w: World, id: string, which: 'gate' | 'comp'): World {
  if (w.state.locked) return w;
  const defaults = DYN_RESET[which];
  if (!defaults) return w;
  return withChannel(w, id, c => {
    const d = c.dyn?.[which] as unknown as Record<string, number | boolean> | undefined;
    if (!d) return;
    for (const [prop, v] of Object.entries(defaults)) d[prop] = v;
    d['on'] = true;
  });
}

// uiLogic/fx: FX slot edits + tap tempo.
// Pure, rune-free logic split out of state/uiLogic.ts (one version of
// the logic lives here; uiLogic.ts re-exports it). DOM-free, node-testable.

import { clamp, tapUpdate } from '../../fixtures/designData.ts';
import { type World, withFx, withState } from './types.ts';

// ---------- FX ----------

export function setFxSel(w: World, i: number): World {
  if (i < 0 || i >= w.fx.length) return w;
  return withState(w, { fxSel: i });
}

export function setFxType(w: World, i: number, type: string): World {
  return withFx(w, i, s => { s.type = type; });
}

export function setFxParam(w: World, i: number, param: number, n: number): World {
  return withFx(w, i, s => {
    s.p[param] = Math.round(clamp(n, 0, 1) * 1000) / 1000;
    if (param === 0) s.bpm = null;
  });
}

export function toggleFxOn(w: World, i: number): World {
  return withFx(w, i, s => { s.on = !s.on; });
}

export function resetFxParam(w: World, i: number, param: number): World {
  return withFx(w, i, s => { s.p[param] = 0.5; });
}

/** Tap tempo with injected tap list + clock (store owns the list). */
export function tapFx(w: World, i: number, prevTaps: number[], now: number): { world: World; taps: number[] } {
  const { taps, bpm, p0 } = tapUpdate(prevTaps, now);
  if (bpm == null || p0 == null) return { world: w, taps };
  const world = withFx(w, i, s => { s.bpm = bpm; s.p[0] = p0; });
  return { world, taps };
}

// uiLogic/eq: EQ edits (not lock-checked in the design).
// Pure, rune-free logic split out of state/uiLogic.ts (one version of
// the logic lives here; uiLogic.ts re-exports it). DOM-free, node-testable.

import { clamp, type EqRange } from '../../fixtures/designData.ts';
import { type World, withChannel } from './types.ts';

// ---------- EQ edits (not lock-checked in the design) ----------

export type EqParam = 'f' | 'g' | 'q';

/** Port of the knob branch of move() for EQ bands (band 0 = HPF). */
export function eqParamFromDrag(param: EqParam, band: number, start: number, dyPx: number, fine: boolean, range?: EqRange | null): number {
  const dy = dyPx * (fine ? 0.25 : 1);
  const [lo, hi] = range ?? (param === 'f' ? [20, band === 0 ? 1000 : 20000] : param === 'g' ? [-15, 15] : [0.3, 16]);
  if (param === 'f') return clamp(start * Math.pow(2, dy / 30), lo, hi);
  if (param === 'g') return Math.round(clamp(start + dy * 0.1, lo, hi) * 10) / 10;
  return Math.round(clamp(start * Math.pow(2, dy / 60), lo, hi) * 100) / 100;
}

/** Port of the node branch of move() (graph drag). */
export function eqNodeFromPos(_band: number, x: number, y: number): { f: number; g: number } {
  const cx = clamp(x, 0, 1), cy = clamp(y, 0, 1);
  return {
    f: 20 * Math.pow(1000, cx),
    g: Math.round((15 - 30 * cy) * 10) / 10,
  };
}

export function setEqParam(w: World, id: string, band: number, param: EqParam, v: number): World {
  return withChannel(w, id, c => {
    if (!c.eq || w.state.locked || !Number.isFinite(v)) return;
    if (band === 0) {
      if (param !== 'f' || !Number.isFinite(c.eq.hpf.f)) return;
      const r = c.eq.hpf.range;
      if (r === null) return;
      c.eq.hpf.f = clamp(v, ...(r ?? [20, 1000] as const));
    }
    else {
      const b = c.eq.bands[band - 1];
      if (!b || !Number.isFinite(b[param])) return;
      const r = b.ranges?.[param];
      if (r === null || (param === 'g' && (b.type === 'LCut' || b.type === 'HCut'))) return;
      b[param] = r ? clamp(v, r[0], r[1]) : v;
    }
  });
}

const RESET_F = [80, 100, 400, 2500, 8000, 12000, 16000];
const RESET_Q = [0.7, 0.7, 1.4, 1.4, 0.7, 1, 1];

/** Port of resetParam (double-tap reset). */
export function resetEqParam(w: World, id: string, band: number, param: EqParam): World {
  const value = param === 'f' ? RESET_F[band] : param === 'g' ? 0 : RESET_Q[band];
  return setEqParam(w, id, band, param, value);
}

export function toggleEqOn(w: World, id: string): World {
  return withChannel(w, id, c => { if (c.eq && !w.state.locked && c.eq.on !== null) c.eq.on = !c.eq.on; });
}

export function toggleEqBand(w: World, id: string, bandIndex: number): World {
  return withChannel(w, id, c => {
    if (!c.eq || w.state.locked) return;
    if (bandIndex === 0) { if (c.eq.hpf.on !== null) c.eq.hpf.on = !c.eq.hpf.on; }
    else {
      const b = c.eq.bands[bandIndex - 1];
      if (b && b.toggleable !== false && b.on !== null) b.on = !b.on;
    }
  });
}

export function flatEq(w: World, id: string): World {
  w.chans[id]?.eq?.bands.forEach((_, i) => { w = setEqParam(w, id, i + 1, 'g', 0); });
  return w;
}

/**
 * Full EQ reset (head-menu Reset row): every band back to the RESET_F/Q
 * tables with gains at 0, HPF at 80 Hz, and EQ switched in so the reset is
 * audible. Band types, per-band bypass states and HPF slope are left alone.
 * Lock-checked: the head menu disables its rows while locked.
 */
export function resetEq(w: World, id: string): World {
  if (w.state.locked) return w;
  const eq = w.chans[id]?.eq;
  if (!eq) return w;
  w = setEqParam(w, id, 0, 'f', 80);
  eq.bands.forEach((_, i) => {
    for (const p of ['f', 'g', 'q'] as const) w = resetEqParam(w, id, i + 1, p);
  });
  return withChannel(w, id, c => { if (c.eq && c.eq.on !== null) c.eq.on = true; });
}

export function setHpfSlope(w: World, id: string, slope: 12 | 24): World {
  return withChannel(w, id, c => { if (c.eq && !w.state.locked && c.eq.hpf.slopeEditable !== false && c.eq.hpf.slope !== null) c.eq.hpf.slope = slope; });
}

export function setEqBandType(w: World, id: string, bandIndex: number, type: import('../../fixtures/designData.ts').EqBandType): World {
  return withChannel(w, id, c => {
    const b = c.eq?.bands[bandIndex - 1];
    if (b && !w.state.locked && b.type !== null && (!b.types || b.types.includes(type))) b.type = type;
  });
}

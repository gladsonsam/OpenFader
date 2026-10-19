// uiLogic/eq: EQ edits (not lock-checked in the design).
// Pure, rune-free logic split out of state/uiLogic.ts (one version of
// the logic lives here; uiLogic.ts re-exports it). DOM-free, node-testable.

import { clamp } from '../../fixtures/designData.ts';
import { type World, withChannel } from './types.ts';

// ---------- EQ edits (not lock-checked in the design) ----------

export type EqParam = 'f' | 'g' | 'q';

/** Port of the knob branch of move() for EQ bands (band 0 = HPF). */
export function eqParamFromDrag(param: EqParam, band: number, start: number, dyPx: number, fine: boolean): number {
  const dy = dyPx * (fine ? 0.25 : 1);
  if (param === 'f') return clamp(start * Math.pow(2, dy / 30), 20, band === 0 ? 1000 : 20000);
  if (param === 'g') return Math.round(clamp(start + dy * 0.1, -15, 15) * 10) / 10;
  return Math.round(clamp(start * Math.pow(2, dy / 60), 0.3, 16) * 100) / 100;
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
    if (!c.eq) return;
    if (band === 0) c.eq.hpf.f = v;
    else {
      const b = c.eq.bands[band - 1];
      if (b) b[param] = v;
    }
  });
}

const RESET_F = [80, 100, 400, 2500, 8000];
const RESET_Q = [0.7, 0.7, 1.4, 1.4, 0.7];

/** Port of resetParam (double-tap reset). */
export function resetEqParam(w: World, id: string, band: number, param: EqParam): World {
  return withChannel(w, id, c => {
    if (!c.eq) return;
    if (band === 0) { c.eq.hpf.f = 80; return; }
    const b = c.eq.bands[band - 1];
    if (!b) return;
    if (param === 'f') b.f = RESET_F[band];
    else if (param === 'g') b.g = 0;
    else b.q = RESET_Q[band];
  });
}

export function toggleEqOn(w: World, id: string): World {
  return withChannel(w, id, c => { if (c.eq) c.eq.on = !c.eq.on; });
}

export function toggleEqBand(w: World, id: string, bandIndex: number): World {
  return withChannel(w, id, c => {
    if (!c.eq) return;
    if (bandIndex === 0) c.eq.hpf.on = !c.eq.hpf.on;
    else {
      const b = c.eq.bands[bandIndex - 1];
      if (b) b.on = !b.on;
    }
  });
}

export function flatEq(w: World, id: string): World {
  return withChannel(w, id, c => { c.eq?.bands.forEach(b => { b.g = 0; }); });
}

/**
 * Full EQ reset (head-menu Reset row): every band back to the RESET_F/Q
 * tables with gains at 0, HPF at 80 Hz, and EQ switched in so the reset is
 * audible. Band types, per-band bypass states and HPF slope are left alone.
 * Lock-checked: the head menu disables its rows while locked.
 */
export function resetEq(w: World, id: string): World {
  if (w.state.locked) return w;
  return withChannel(w, id, c => {
    if (!c.eq) return;
    c.eq.on = true;
    c.eq.hpf.f = 80;
    c.eq.bands.forEach((b, i) => {
      const band = i + 1;
      const f = RESET_F[band];
      if (f !== undefined) b.f = f;
      b.g = 0;
      const q = RESET_Q[band];
      if (q !== undefined) b.q = q;
    });
  });
}

export function setHpfSlope(w: World, id: string, slope: 12 | 24): World {
  return withChannel(w, id, c => { if (c.eq) c.eq.hpf.slope = slope; });
}

export function setEqBandType(w: World, id: string, bandIndex: number, type: 'LS' | 'PEQ' | 'HS'): World {
  return withChannel(w, id, c => {
    const b = c.eq?.bands[bandIndex];
    if (b) b.type = type;
  });
}

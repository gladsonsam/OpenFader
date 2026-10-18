// RTA overlay helpers for the EQ graph: turn the engine's per-channel
// spectrum frames (64 bins, peak-normalised 0..1) into an SVG area path in
// EQ_GRAPH_W x EQ_GRAPH_H coordinates, with peak-hold + decay smoothing.
//
// Honesty note: these bins are NOT audio frequencies. The desk (XR18)
// streams ~20 Hz per-channel meter envelopes, and the engine FFTs the
// trailing 100 ms of that envelope history (RtaProcessor::spectrum in
// crates/core/src/rta.rs). So the overlay pumps with real signal dynamics
// (silence reads flat, and the graph hides it) but the bin order is
// envelope-modulation content spread across the log axis as a display
// convention — a live activity backdrop, not a calibrated RTA. True
// per-frequency RTA would need audio taps the desk does not stream.
//
// Rune-free and DOM-free: node-testable like the rest of this folder
// (no test file: repo rule bans new frontend tests).

import { EQ_GRAPH_H, EQ_GRAPH_W } from './eqModel.ts';

/** Engine spectrum width (SPECTRUM_BINS in crates/core/src/rta.rs). */
export const RTA_BINS = 64;

/** Log-frequency display span (matches the EQ graph axis). */
export const RTA_FMIN = 20;
export const RTA_FMAX = 20000;

/** Magnitude below which the channel counts as silent: overlay hides. */
export const RTA_SILENCE = 0.02;

/** Peak-hold decay per overlay tick (~120 ms). */
export const RTA_DECAY = 0.88;

/** Overlay repaint interval (slower when reduced motion is on). */
export const RTA_TICK_MS = 120;
export const RTA_TICK_MS_CALM = 1000;

/** Peak overlay height in viewBox units (leaves headroom for the curve). */
export const RTA_PEAK_H = 190;

const xOf = (f: number, w: number): number =>
  (Math.log(f / RTA_FMIN) / Math.log(RTA_FMAX / RTA_FMIN)) * w;

/** True when any bin carries real signal (otherwise the graph hides). */
export function rtaHasSignal(bins: ArrayLike<number>): boolean {
  for (let i = 0; i < bins.length; i++) {
    const v = bins[i];
    if (typeof v === 'number' && Number.isFinite(v) && v > RTA_SILENCE) return true;
  }
  return false;
}

/**
 * Peak-hold smoothing: instant attack to the fresh bins, exponential decay
 * on release. Always returns a NEW array — never mutates the engine's
 * cached frame (engineWorld reuses the buffer in place).
 */
export function rtaSmooth(
  prev: ArrayLike<number> | null,
  next: ArrayLike<number>,
  decay: number = RTA_DECAY,
): Float32Array {
  const n = Math.min(next.length, RTA_BINS);
  const out = new Float32Array(RTA_BINS);
  for (let i = 0; i < RTA_BINS; i++) {
    const fresh = i < n ? next[i] : 0;
    const held = prev && i < prev.length ? (prev[i] as number) * decay : 0;
    out[i] = Math.max(fresh, held);
  }
  return out;
}

/**
 * Filled area path for the overlay in EQ_GRAPH_W x EQ_GRAPH_H coordinates.
 * Bins spread log-wise across 20 Hz..20 kHz; magnitude (sqrt-lifted so
 * small values stay visible) rises from the bottom edge. Returns '' when
 * there is no signal — the caller renders nothing (honest hide).
 */
export function rtaAreaPath(
  bins: ArrayLike<number>,
  w: number = EQ_GRAPH_W,
  h: number = EQ_GRAPH_H,
): string {
  if (!rtaHasSignal(bins)) return '';
  const n = Math.min(bins.length, RTA_BINS);
  if (n === 0) return '';
  let d = '';
  for (let i = 0; i < n; i++) {
    const f = RTA_FMIN * Math.pow(RTA_FMAX / RTA_FMIN, n === 1 ? 1 : i / (n - 1));
    const m = Math.min(Math.max(bins[i] as number, 0), 1);
    const top = h - Math.sqrt(m) * RTA_PEAK_H;
    d += (i === 0 ? 'M' : ' L') + xOf(f, w).toFixed(1) + ' ' + top.toFixed(1);
  }
  const lastF = RTA_FMIN * Math.pow(RTA_FMAX / RTA_FMIN, 1);
  d += ' L' + xOf(lastF, w).toFixed(1) + ' ' + h + ' L0 ' + h + ' Z';
  return d;
}

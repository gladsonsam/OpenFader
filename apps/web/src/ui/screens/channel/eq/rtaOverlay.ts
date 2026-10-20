// Simulator envelope helpers and provider audio-spectrum rendering.
// Hardware bins preserve dB precision and explicit provider band edges. They
// never pass through the legacy envelope's 64-bin normalization/smoothing.

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

/** Measured audio bars, using provider frequency edges and a fixed dB scale.
 * The height represents −90..0 dB; no peak normalization or invented bins.
 */
export function audioRtaAreaPath(
  binsDb: ArrayLike<number>, edges: ArrayLike<number>,
  w: number = EQ_GRAPH_W, h: number = EQ_GRAPH_H,
): string {
  if (!binsDb.length || edges.length !== binsDb.length + 1) return '';
  let d = '';
  for (let i = 0; i < binsDb.length; i++) {
    const lo = Math.max(RTA_FMIN, edges[i]!);
    const hi = Math.min(RTA_FMAX, edges[i + 1]!);
    if (lo >= hi) continue;
    const y = h - Math.max(0, Math.min(1, (binsDb[i]! + 90) / 90)) * RTA_PEAK_H;
    const x0 = xOf(lo, w).toFixed(2), x1 = xOf(hi, w).toFixed(2);
    d += `M${x0} ${h} L${x0} ${y.toFixed(2)} L${x1} ${y.toFixed(2)} L${x1} ${h} Z `;
  }
  return d;
}

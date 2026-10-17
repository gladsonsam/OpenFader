// Shared parametric-EQ curve math for EqGraph (canvas) + FatStrip (SVG thumb).
// Single source of truth — no duplication across components.
//
// Filter-type word (`eqN_type`) provenance (read-only survey, 2026-10-17):
// - DL32S wire (crates/proto-dl32s/src/addrs.rs `input_eq`): bands 2/3/4
//   carry a `type` word (Encoding::Enum — the driver casts any finite float
//   to u32, no range check); band 1 has gain/freq/Q ONLY, no type word.
// - sim (crates/sim/src/lib.rs `patterned_param_kind`): `eq{2..4}_type`
//   accepted as enum-as-float (any finite value); `eq1_type` is unknown.
// - UI (apps/web/src/lib/sections.ts, mirrored in ui/app.js): type selects
//   offer opts [0, 1, 2, 3], def 0.
// - xair (crates/proto-xair/src/lib.rs `get_param`): NO eq param support —
//   any eq field bails unknown, so shelves stay indicative until a desk
//   mapping lands.
// No driver or doc assigns meanings to 0..3, so the mapping below is the
// UI-side convention, chosen to keep today's rendering stable:
//   0 = bell/PEQ (default — what every band renders as today),
//   1 = low shelf, 2 = high shelf, 3 = reserved (falls back to bell).
// The backend always receives the exact gain/freq/Q/type numbers; only the
// indicative curve is approximate for unmapped values.
//
// Detail-owner note: Detail's `eqN_type` enum select currently pushes via
// onParam without syncing `bands[n].type`, so the graph/thumb keep showing
// bells until Detail copies the select into its eqBands state.

export interface EqBand {
  gain: number; // dB, -20..+20
  freq: number; // Hz, 20..20000
  q: number; // 0.1..20
  /** Filter type word (`eqN_type`): 0 bell, 1 low shelf, 2 high shelf. Omitted = bell. */
  type?: number;
}

/** `eqN_type` convention (see header): bell/PEQ default. */
export const EQ_TYPE_BELL = 0;
/** `eqN_type` convention (see header): low shelf. */
export const EQ_TYPE_LOW_SHELF = 1;
/** `eqN_type` convention (see header): high shelf. */
export const EQ_TYPE_HIGH_SHELF = 2;

export const FMIN = 20;
export const FMAX = 20000;
export const GMIN = -20;
export const GMAX = 20;
/** Display range for graphs (values clamp to GMIN/GMAX on write). */
export const GSHOW = 15;

export const clampGain = (g: number): number => Math.min(GMAX, Math.max(GMIN, g));
export const clampFreq = (f: number): number =>
  Math.min(FMAX, Math.max(FMIN, Number.isFinite(f) ? f : 1000));
export const clampQ = (q: number): number => Math.min(20, Math.max(0.1, q));

/** Engineering-unit defaults mirror sections.ts `EQ bands 1-4` defs. */
export const EQ_DEFAULTS: EqBand[] = [
  { gain: 0, freq: 100, q: 1 },
  { gain: 0, freq: 2000, q: 1 },
  { gain: 0, freq: 3000, q: 1 },
  { gain: 0, freq: 12000, q: 1 },
];

/** Log-frequency position 0..1 for f Hz. */
export const freqToT = (f: number): number =>
  Math.log(Math.min(FMAX, Math.max(FMIN, f)) / FMIN) / Math.log(FMAX / FMIN);

/** Hz for log position 0..1. */
export const tToFreq = (t: number): number =>
  FMIN * Math.pow(FMAX / FMIN, Math.min(1, Math.max(0, t)));

/** One band's contribution in dB: bell peaks at center, shelves hold the
 *  shelf side and relax to 0 on the far side (gain/2 at center). Q sets the
 *  bell width / shelf transition steepness. Unmapped `type` (incl. reserved
 *  3) falls back to bell — see header. */
export function bandDb(b: EqBand, f: number): number {
  const t = b.type ?? EQ_TYPE_BELL;
  if (t === EQ_TYPE_LOW_SHELF) return shelfDb(b, f, true);
  if (t === EQ_TYPE_HIGH_SHELF) return shelfDb(b, f, false);
  const q = Math.max(0.1, b.q || 1);
  const x = Math.log2(Math.max(1, f) / Math.max(1, b.freq || 1000));
  return b.gain / (1 + Math.pow(2 * q * x, 2));
}

/** Logistic shelf in log-frequency: full gain on the shelf side. */
function shelfDb(b: EqBand, f: number, low: boolean): number {
  const q = Math.max(0.1, b.q || 1);
  const x = Math.log2(Math.max(1, f) / Math.max(1, b.freq || 1000));
  return b.gain / (1 + Math.pow(2, 2 * q * (low ? x : -x)));
}

/** Summed curve in dB at f Hz. */
export function curveDb(bands: EqBand[], f: number): number {
  let s = 0;
  for (const b of bands) s += bandDb(b, f);
  return s;
}

/** Ball position in a w×h box (y: +dB up, x: log freq). */
export function bandPoint(b: EqBand, w: number, h: number, gshow = GSHOW): { x: number; y: number } {
  return {
    x: freqToT(b.freq) * w,
    y: (1 - (b.gain + gshow) / (2 * gshow)) * h,
  };
}

/** SVG path `d` for the summed curve in a w×h box. */
export function curvePath(bands: EqBand[], w: number, h: number, gshow = GSHOW, steps = 72): string {
  let d = '';
  for (let i = 0; i <= steps; i++) {
    const f = tToFreq(i / steps);
    const y = (1 - (curveDb(bands, f) + gshow) / (2 * gshow)) * h;
    d += `${i === 0 ? 'M' : 'L'}${(freqToT(f) * w).toFixed(1)},${Math.min(h, Math.max(0, y)).toFixed(1)}`;
  }
  return d;
}

/** Resample relay bins (linear, low→high) onto n log-spaced columns. */
export function resampleBins(bins: number[], n: number): number[] {
  const out = new Array<number>(n);
  for (let i = 0; i < n; i++) {
    const f = tToFreq(n === 1 ? 0 : i / (n - 1));
    const src = bins[Math.min(bins.length - 1, Math.floor(freqToT(f) * bins.length))];
    out[i] = Math.min(1, Math.max(0, Number(src) || 0));
  }
  return out;
}

// Deterministic per-column shape so the RTA fallback reads as a spectrum
// decaying with frequency (no randomness — same columns every frame).
function shapeAt(i: number, n: number): number {
  const x = Math.sin(i * 12.9898) * 43758.5453;
  const pseudo = x - Math.floor(x);
  return Math.exp(-i / (n * 0.28)) * (0.55 + 0.45 * pseudo);
}

/**
 * Levels-derived RTA fallback (mirrors Spectrum.svelte): interpolated columns
 * from the live meter level until the relay sends real spectra.
 */
export function fallbackBins(level: number, n: number): number[] {
  const out = new Array<number>(n);
  for (let i = 0; i < n; i++) out[i] = Math.min(1, level * 1.6 * shapeAt(i, n));
  return out;
}

/** Pretty freq: 100 → "100", 1200 → "1.2k". */
export function fmtFreq(f: number): string {
  const r = Math.round(f);
  return r >= 1000 ? `${(r / 1000).toFixed(r >= 10000 ? 0 : 1).replace(/\.0$/, '')}k` : `${r}`;
}

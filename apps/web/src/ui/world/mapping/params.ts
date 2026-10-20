// W2a mapping: engine catalogue ids/engineering units <-> design channel model.
//
// Pure functions, both directions, across the whole range (see params.test.ts
// round trips). Clamping is explicit; lossy folds are documented, not hidden.
//
// Engine facts (crates/core/src/params.rs CATALOGUE; enum order from
// docs/xair-protocol.md:64 `LCut,LShv,PEQ,VEQ,HShv,HCut`):
//   eqN_type  enum 0..5   hpf_slope enum 0..2 (X32-only, meaning UNVERIFIED)
//   gate_* / comp_* / sendN_* / input ids as listed in RANGE_NOTES below.
//
// MISMATCHES vs the design knob specs (ui/fixtures/designData.ts spec()):
//   gate_range  engine 0..60    vs design 0..80      -> clamp, top 20 dB unreachable
//   gate_att    engine 0..120ms vs design 0.05..50ms -> clamp both ends
//   gate_hold   engine 0..500ms vs design 1..2000ms  -> clamp both ends
//   gate_rel    engine 5..1000  vs design 5..4000ms  -> clamp top
//   comp_ratio  engine 1.1..100 vs design 1..20      -> clamp both ends (engine is
//                a plain number, not stepped)
//   comp_knee   engine 0..5     vs design 0..12      -> clamp top
//   comp_att    engine 0..300ms vs design 0.1..100ms -> clamp both ends
//   comp_hold   engine 0..500ms vs design NO KNOB    -> gap: no design control
//   comp_rel    engine 10..1000 vs design 5..2000ms  -> clamp both ends
//   gain        engine -12..60  vs design 0..60dB    -> clamp bottom (-12..0 unreachable)
//   delay_ms    engine 0..100ms vs design 0..500ms   -> clamp top
//   eq types    all six engine types are preserved in both directions
//   hpf_slope   engine 0..2 (meaning UNVERIFIED-ON-RIG; assumed 0=12,1=18,2=24)
//                vs design 12|24 -> 18 folds to 12 on read
//   send tap    engine 0..5 IN/PREEQ/POSTEQ/PRE/POST/GRP (desk-verified
//                2026-10-20: bus slots 1-6 accept all six, FX slots 7-10
//                refuse 5=GRP; defaults 2 on buses, 4 on FX; design fixture
//                sendPre booleans are offline-only and unrelated)
// Exact (no mismatch): pan (-100..100 <-> -1..1), trim, comp_thr, comp_makeup,
// phantom/polarity/delay_on, fader wire, name/source, mute.

import { XAIR_LAW, type FaderLaw } from './faderLaw.ts';

export type DesignEqType = 'LCut' | 'LS' | 'PEQ' | 'VEQ' | 'HS' | 'HCut';

/** Engine eqN_type order (docs/xair-protocol.md:64). */
export const EQ_TYPE_NAMES = ['LCut', 'LShv', 'PEQ', 'VEQ', 'HShv', 'HCut'] as const;

/** Preserve all six wire types. An unknown enum is never a made-up bell. */
export function engineEqTypeToDesign(t: number): DesignEqType | '' {
  return (['LCut', 'LS', 'PEQ', 'VEQ', 'HS', 'HCut'] as const)[t] ?? '';
}

export function designEqTypeToEngine(t: DesignEqType): number {
  return ['LCut', 'LS', 'PEQ', 'VEQ', 'HS', 'HCut'].indexOf(t);
}

/**
 * Engine hpf_slope 0..2 -> design 12|24. The enum meaning is
 * UNVERIFIED-ON-RIG (assumed 0=12, 1=18, 2=24 dB/oct); 18 folds to 12.
 */
export function engineSlopeToDesign(s: number): 12 | 24 {
  return Math.round(s) >= 2 ? 24 : 12;
}

/** Design 12|24 -> engine enum (12->0, 24->2). */
export function designSlopeToEngine(s: 12 | 24): number {
  return s === 24 ? 2 : 0;
}

/** Engine pan -100..100 <-> design pan -1..1 (exact, /100). */
export function enginePanToDesign(p: number): number {
  return Math.round((Math.min(100, Math.max(-100, p)) / 100) * 100) / 100;
}

export function designPanToEngine(p: number): number {
  return Math.min(100, Math.max(-100, Math.round(Math.min(1, Math.max(-1, p)) * 100)));
}

/**
 * Engine send level dB (-90..+10) <-> design 0..1 send value via the fader
 * law (the design's sendAria reads the same curve, so dB text agrees).
 */
export function sendDbToDesign01(db: number, law: FaderLaw = XAIR_LAW): number {
  if (!Number.isFinite(db)) return 0;
  return Math.min(1, Math.max(0, law.fromDb(Math.min(10, Math.max(-90, db)))));
}

export function design01ToSendDb(v: number, law: FaderLaw = XAIR_LAW): number {
  if (!Number.isFinite(v) || v <= 0) return -90;
  return law.toDb(Math.min(1, Math.max(0, v)));
}

/** Engine sendN_tap enum -> design pre flag (fixture/offline only: the live
 *  sends tab reads the desk tap through the overlay instead). Kept for the
 *  boolean fixture `sendPre` path; NOT the desk truth (see SEND_TAP_LABELS). */
export function tapToPre(tap: number, driver?: string): boolean {
  return driver === 'xair' ? tap >= 0 && tap <= 3 : Math.round(tap) === 0;
}

export function preToTap(pre: boolean, driver?: string): number {
  return driver === 'xair' ? pre ? 3 : 4 : pre ? 0 : 1;
}

/**
 * Desk send-tap point names (XR18 `/ch/NN/mix/MM/tap` int `0..=5`).
 * Rig-verified 2026-10-20: 0 IN, 1 PREEQ, 2 POSTEQ, 3 PRE (pre-fader),
 * 4 POST (post-fader), 5 GRP = the subgroup pickoff (mixer control software's
 * "subgroup" option — there is NO separate per-send subgroup leaf; every
 * `/mix/MM/subgroup|grp|assign` candidate is silent on the desk).
 */
export const SEND_TAP_LABELS: readonly string[] = [
  'IN',
  'Pre-EQ',
  'Post-EQ',
  'Pre',
  'Post',
  'Sub-GRP',
];

/** Tap enum value -> short button label (`IN`, `Pre-EQ`, …; `?` when unknown). */
export function tapLabel(tap: number): string {
  const i = Math.round(tap);
  return Number.isInteger(i) && i >= 0 && i < SEND_TAP_LABELS.length
    ? SEND_TAP_LABELS[i]!
    : '?';
}

/**
 * Legal tap choices for a 1-based generic send slot. The XR18 (`xair`
 * driver) is 6+4: bus slots 1-6 accept the full six-point set (writes
 * 0..5 read back exact), FX slots 7-10 refuse 5=GRP (the write is
 * ignored, the slot keeps its value — verified 2026-10-20 on muted ch16
 * slots 07/08/10). Every other driver keeps the legacy generic design
 * pre/post pair (0/1).
 */
export function tapOptionsForSlot(driver: string, n: number): number[] {
  if (driver === 'xair' && Number.isInteger(n)) {
    if (n >= 1 && n <= 6) return [0, 1, 2, 3, 4, 5];
    if (n >= 7 && n <= 10) return [0, 1, 2, 3, 4];
  }
  return [0, 1];
}

/**
 * Desk destination label for a 1-based generic send slot (sends-tab rows,
 * SOF aria). The XR18 (`xair` driver) is a 6+4 desk: slots 1-6 feed the six
 * bus masters, 7-10 the four FX send masters (firmware `mix/01..10`;
 * owner-rig verified 2026-10-20 that `/mix/11..16` leaves and containers
 * are silent on channel and bus roots, so slots past 10 never render on
 * xair). Every other driver keeps the generic design `Mix N` label.
 */
export function sendDestName(driver: string, n: number): string {
  if (driver === 'xair' && Number.isInteger(n)) {
    if (n >= 1 && n <= 6) return `Bus ${n}`;
    if (n >= 7 && n <= 10) return `FX ${n - 6}`;
  }
  return `Mix ${n}`;
}

export function clamp(v: number, lo: number, hi: number): number {
  if (!Number.isFinite(v)) return lo;
  return Math.min(hi, Math.max(lo, v));
}

/** Design knob ranges (ui/fixtures/designData.ts spec()), null = no knob. */
export const DESIGN_RANGES: Record<string, readonly [number, number] | null> = {
  gain: [0, 60],
  trim: [-18, 18],
  delay: [0, 500],
  gthr: [-80, 0],
  range: [0, 80],
  gatt: [0.05, 50],
  hold: [1, 2000],
  grel: [5, 4000],
  cthr: [-60, 0],
  ratio: [1, 20],
  catt: [0.1, 100],
  crel: [5, 2000],
  knee: [0, 12],
  makeup: [0, 24],
};

/** Engine catalogue ranges (crates/core/src/params.rs), keyed by design knob. */
export const ENGINE_RANGES: Record<string, readonly [number, number] | null> = {
  gain: [-12, 60],
  trim: [-18, 18],
  delay: [0, 100],
  gthr: [-80, 0],
  range: [0, 60],
  gatt: [0, 120],
  hold: [0, 500],
  grel: [5, 1000],
  cthr: [-60, 0],
  ratio: [1.1, 100],
  catt: [0, 300],
  crel: [10, 1000],
  knee: [0, 5],
  makeup: [0, 24],
};

export interface RangeNote {
  designKnob: string;
  engineId: string;
  engine: readonly [number, number] | null;
  design: readonly [number, number] | null;
  match: boolean;
  note: string;
}

const RANGE_TABLE: Array<[string, string, string]> = [
  ['gain', 'gain', 'design floor 0 dB; engine -12..0 dB unreachable from the knob'],
  ['trim', 'trim', 'exact'],
  ['delay', 'delay_ms', 'engine caps at 100 ms; design knob runs to 500 ms'],
  ['gthr', 'gate_thr', 'exact'],
  ['range', 'gate_range', 'engine caps at 60 dB; design knob runs to 80 dB'],
  ['gatt', 'gate_att', 'engine 0..120 ms vs design 0.05..50 ms (log knob)'],
  ['hold', 'gate_hold', 'engine 0..500 ms vs design 1..2000 ms (log knob)'],
  ['grel', 'gate_rel', 'engine caps at 1000 ms; design knob runs to 4000 ms'],
  ['cthr', 'comp_thr', 'exact'],
  ['ratio', 'comp_ratio', 'engine 1.1..100 (plain number) vs design 1..20 (log knob)'],
  ['catt', 'comp_att', 'engine 0..300 ms vs design 0.1..100 ms (log knob)'],
  ['crel', 'comp_rel', 'engine 10..1000 ms vs design 5..2000 ms (log knob)'],
  ['knee', 'comp_knee', 'engine caps at 5 dB; design knob runs to 12 dB'],
  ['makeup', 'comp_makeup', 'exact'],
];

/** Full range report; `match=false` rows are the mapping mismatches. */
export function rangeReport(): RangeNote[] {
  return RANGE_TABLE.map(([knob, id, note]) => {
    const engine = ENGINE_RANGES[knob] ?? null;
    const design = DESIGN_RANGES[knob] ?? null;
    const match =
      note === 'exact' ||
      (!!engine &&
        !!design &&
        engine[0] === design[0] &&
        engine[1] === design[1]);
    return { designKnob: knob, engineId: id, engine, design, match, note };
  });
}

/** Only the mismatching rows (what the verification report must list). */
export function rangeMismatches(): RangeNote[] {
  return rangeReport().filter((r) => !r.match);
}

/** Design-knob <-> engine-id wiring for the channel tabs. */
export const KNOB_TO_ENGINE: Record<'gate' | 'comp', Record<string, string>> = {
  gate: { thr: 'gate_thr', range: 'gate_range', att: 'gate_att', hold: 'gate_hold', rel: 'gate_rel' },
  comp: { thr: 'comp_thr', ratio: 'comp_ratio', att: 'comp_att', rel: 'comp_rel', knee: 'comp_knee', makeup: 'comp_makeup' },
};

/**
 * Desk colour 0..15 -> name-bar hex. UI-chosen palette anchored on the design
 * bank colours (sim color_default 2=green, 6=cyan, 4=blue, 11=magenta,
 * 0=red/DCA); NOT desk-verified — the desk only sends the index.
 */
export const COLOR_HEX: readonly string[] = [
  '#dc2626', // 0 red (DCA default)
  '#f97316', // 1 orange
  '#5cdb5c', // 2 green (input default)
  '#a3e635', // 3 lime
  '#2563eb', // 4 blue (fxsnd default)
  '#7c3aed', // 5 violet
  '#22d3e6', // 6 cyan (bus/fxret default)
  '#2dd4bf', // 7 teal
  '#eab308', // 8 yellow
  '#f43f5e', // 9 rose
  '#f472b6', // 10 pink
  '#c026d3', // 11 magenta (mtx/main default)
  '#94a3b8', // 12 slate
  '#fb7185', // 13 light red
  '#86efac', // 14 light green
  '#e2e8f0', // 15 near white
];

export function colorToHex(c: number): string {
  const i = Math.round(c);
  if (!Number.isInteger(i) || i < 0 || i >= COLOR_HEX.length) return '#64748b';
  return COLOR_HEX[i]!;
}

/** Design bank palette by derived bank key prefix (matches designData.ts). */
export function bankPalette(key: string): { colour: string; text: string } {
  if (key.startsWith('mix')) return { colour: '#22d3e6', text: '#062a30' };
  if (key.startsWith('grp') || key.startsWith('fxsnd')) return { colour: '#2563eb', text: '#ffffff' };
  if (key.startsWith('mtx') || key.startsWith('main')) return { colour: '#c026d3', text: '#ffffff' };
  if (key.startsWith('dca')) return { colour: '#dc2626', text: '#ffffff' };
  return { colour: '#5cdb5c', text: '#10240f' };
}

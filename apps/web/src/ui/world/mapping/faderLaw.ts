// W2a mapping: fader laws.
//
// The design fader law (ui/fixtures/designData.ts: law/posToDb/dbToPos) and
// the engine/driver fader law (formerly apps/web/src/v3/faderLaw.ts: the X-Air/X32
// "level" curve) were written from different sources. This module keeps both,
// measures whether they are the same curve, and exposes the law the World
// must follow (the one named by caps; default X-Air).
//
// REPORT (measured in faderLaw.test.ts, table below):
// Both laws agree EXACTLY at every design joint (0.125/-50, 0.25/-30,
// 0.375/-20, 0.5/-10, 0.625/-5, 0.75/0, 0.875/+5, 1/+10) and on every
// segment above f=0.125 — the design's extra mid-joints sit exactly on the
// X-Air line segments (slopes 160 / 80 / 40 dB per unit). They differ ONLY
// below f=0.125: X-Air runs 480 dB/unit to (0.0625, -60) then 160 dB/unit,
// while the design runs a single 320 dB/unit segment from (0, -90). Worst
// case is f=0.0625: design -70 dB vs X-Air -60 dB (10 dB apart). Floors also
// differ: design reads -inf for pos <= 0.003, X-Air only at f <= 0.
//
// Consequence: the World exposes faderToDb/dbToFader from the X-Air law
// (caps-named law, default `xair`), and the UI Fader primitive + dB readouts
// must follow the WORLD's law (not the fixture law) when bound to the
// engine, or the readout will disagree with the engine round-trip by up to
// 10 dB at the bottom of the travel. Fixture mode keeps the design law, so
// no pixel changes there (W1 owns that binding).

export interface FaderLaw {
  id: string;
  label: string;
  toDb(f: number): number;
  fromDb(db: number): number;
  floorDb: number;
  topDb: number;
}

export const TOP_DB = 10;
export const FLOOR_DB = -90;

/** Design law joints (designData.ts law()). */
const DESIGN_POINTS: ReadonlyArray<readonly [number, number]> = [
  [0, -90],
  [0.125, -50],
  [0.25, -30],
  [0.375, -20],
  [0.5, -10],
  [0.625, -5],
  [0.75, 0],
  [0.875, 5],
  [1, 10],
];

function designToDb(f: number): number {
  if (!Number.isFinite(f) || f <= 0.003) return -Infinity;
  const T = DESIGN_POINTS;
  for (let i = 1; i < T.length; i++) {
    if (f <= T[i]![0]) {
      const a = T[i - 1]!;
      const b = T[i]!;
      return a[1] + ((b[1] - a[1]) * (f - a[0])) / (b[0] - a[0]);
    }
  }
  return TOP_DB;
}

function designFromDb(db: number): number {
  if (!Number.isFinite(db) || db <= FLOOR_DB) return 0;
  const T = DESIGN_POINTS;
  for (let i = 1; i < T.length; i++) {
    if (db <= T[i]![1]) {
      const a = T[i - 1]!;
      const b = T[i]!;
      return a[0] + ((b[0] - a[0]) * (db - a[1])) / (b[1] - a[1]);
    }
  }
  return 1;
}

/** X-Air / X32 "level" curve (v3/faderLaw.ts, joints 0.0625/-60 … 1/+10). */
function xairToDb(f: number): number {
  if (!Number.isFinite(f) || f <= 0) return -Infinity;
  if (f >= 0.5) return 40 * f - 30;
  if (f >= 0.25) return 80 * f - 50;
  if (f >= 0.0625) return 160 * f - 70;
  return 480 * f - 90;
}

function xairFromDb(db: number): number {
  if (!Number.isFinite(db) || db <= FLOOR_DB) return 0;
  if (db >= TOP_DB) return 1;
  if (db >= -10) return 0.5 + (db + 10) / 40;
  if (db >= -30) return 0.25 + (db + 30) / 80;
  if (db >= -60) return 0.0625 + (db + 60) / 160;
  return (db - FLOOR_DB) / 480;
}

export const DESIGN_LAW: FaderLaw = {
  id: 'design',
  label: 'Design fixture law',
  toDb: designToDb,
  fromDb: designFromDb,
  floorDb: FLOOR_DB,
  topDb: TOP_DB,
};

export const XAIR_LAW: FaderLaw = {
  id: 'xair',
  label: 'X-Air / X32',
  toDb: xairToDb,
  fromDb: xairFromDb,
  floorDb: FLOOR_DB,
  topDb: TOP_DB,
};

/** DL32S / DigiMixer fader scale (uniform 9-segment over its dB table —
 *  matches `DL32S_FADER_SCALE` in `crates/proto-dl32s`, so the readout
 *  agrees with the engine round-trip exactly). Joints:
 *  0/-120, 1/9/-60, 2/9/-40, 3/9/-30, 4/9/-20, 5/9/-10, 6/9/-5, 7/9/0,
 *  8/9/+5, 1/+10. */
const DL32S_POINTS: ReadonlyArray<readonly [number, number]> = [
  [0, -120],
  [1 / 9, -60],
  [2 / 9, -40],
  [3 / 9, -30],
  [4 / 9, -20],
  [5 / 9, -10],
  [6 / 9, -5],
  [7 / 9, 0],
  [8 / 9, 5],
  [1, 10],
];

function dl32sToDb(f: number): number {
  if (!Number.isFinite(f) || f <= 0) return -Infinity;
  const T = DL32S_POINTS;
  for (let i = 1; i < T.length; i++) {
    if (f <= T[i]![0]) {
      const a = T[i - 1]!;
      const b = T[i]!;
      return a[1] + ((b[1] - a[1]) * (f - a[0])) / (b[0] - a[0]);
    }
  }
  return TOP_DB;
}

function dl32sFromDb(db: number): number {
  if (!Number.isFinite(db) || db <= -120) return 0;
  if (db >= TOP_DB) return 1;
  const T = DL32S_POINTS;
  for (let i = 1; i < T.length; i++) {
    if (db <= T[i]![1]) {
      const a = T[i - 1]!;
      const b = T[i]!;
      return a[0] + ((b[0] - a[0]) * (db - a[1])) / (b[1] - a[1]);
    }
  }
  return 1;
}

export const DL32S_LAW: FaderLaw = {
  id: 'dl32s',
  label: 'Mackie DL32S',
  toDb: dl32sToDb,
  fromDb: dl32sFromDb,
  floorDb: -120,
  topDb: TOP_DB,
};

const LAWS: Record<string, FaderLaw> = { design: DESIGN_LAW, xair: XAIR_LAW, dl32s: DL32S_LAW };

/**
 * Law named by caps (driver id → law). The relay does not ship a law id yet,
 * so each driver resolves to its own measured law (`design` is
 * fixture-only). Unknown drivers fall back to X-Air.
 */
export function lawForDriver(driver?: string | null): FaderLaw {
  if (driver && Object.prototype.hasOwnProperty.call(LAWS, driver)) return LAWS[driver]!;
  return XAIR_LAW;
}

/** dB readout: one decimal, `-inf` at/below the floor. */
export function formatDb(db: number): string {
  if (!Number.isFinite(db) || db <= FLOOR_DB) return '-inf';
  if (Math.round(db * 10) / 10 === 0) return '0.0';
  return `${db >= 0 ? '+' : ''}${db.toFixed(1)}`;
}

/** Max |design − xair| dB over a sweep (the mismatch report number). */
export function maxLawDeviation(samples = 1001): { maxDb: number; atF: number } {
  let maxDb = 0;
  let atF = 0;
  for (let i = 1; i <= samples; i++) {
    const f = i / samples;
    const a = DESIGN_LAW.toDb(f);
    const b = XAIR_LAW.toDb(f);
    // Below f=0.003 the design reads -inf (floor rule) while X-Air stays
    // finite until 0: that region is excluded here and covered by the floor
    // tests (design <= 0.003 -> -inf; xair <= 0 -> -inf).
    if (!Number.isFinite(a) || !Number.isFinite(b)) continue;
    const d = Math.abs(a - b);
    if (d > maxDb) {
      maxDb = d;
      atF = f;
    }
  }
  return { maxDb, atF };
}

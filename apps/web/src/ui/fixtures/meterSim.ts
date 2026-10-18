// D2 Data package: deterministic meter generator, a port of the design's
// stepMeters (docs/design/src/design-logic.js.txt) plus a frozen mode for
// fixture screenshots.
//
// Design behaviour ported exactly:
//   target = -42 + act*40 + sin(t*(1.3+act*2)+ph)*6 + (rand*10-7)   (act>0.12)
//   cur += (target-cur)*0.7 when rising, else max(-90, cur-1.4)
//   clip latched 1.5 s when cur > -0.3
//   peak = { v: cur, until: now+1500 } when cur >= peak or now > until
// Peak-hold is a 1.5 s setting: tracking always runs (as in the design); the
// `peakHold` display flag only decides whether screens draw the peak line.
//
// Realtime rules (SPEC 2a): everything is preallocated typed arrays, indexed
// by channel slot. step() allocates nothing and does no object lookups.

import { clamp } from './designData.ts';

export const PEAK_HOLD_MS = 1500;
export const CLIP_HOLD_MS = 1500;
export const ACTIVE_THRESHOLD = 0.12;

export class MeterSim {
  readonly ids: string[];
  private readonly indexOf: Map<string, number>;
  /** Current smoothed level in dBFS. */
  readonly level: Float32Array;
  /** Peak-hold level in dBFS. */
  readonly peak: Float32Array;
  /** Timestamps (ms) until which peak/clip stay latched. */
  readonly peakUntil: Float64Array;
  readonly clipUntil: Float64Array;
  /** Per-slot activity + phase, copied in via bind(). */
  private readonly act: Float32Array;
  private readonly ph: Float32Array;

  constructor(ids: string[]) {
    this.ids = ids.slice();
    this.indexOf = new Map(ids.map((id, i) => [id, i]));
    const n = ids.length;
    this.level = new Float32Array(n).fill(-90);
    this.peak = new Float32Array(n).fill(-90);
    this.peakUntil = new Float64Array(n);
    this.clipUntil = new Float64Array(n);
    this.act = new Float32Array(n);
    this.ph = new Float32Array(n);
  }

  get size(): number {
    return this.ids.length;
  }

  slot(id: string): number {
    const i = this.indexOf.get(id);
    return i === undefined ? -1 : i;
  }

  /** Copy activity/phase for all slots (call when channels change, not per frame). */
  bind(getAct: (id: string) => number, getPh: (id: string) => number): void {
    for (let i = 0; i < this.ids.length; i++) {
      const id = this.ids[i];
      this.act[i] = getAct(id);
      this.ph[i] = getPh(id);
    }
  }

  /**
   * Advance all meters one tick. `rand` defaults to Math.random (live mode);
   * pass a seeded PRNG for deterministic tests. Allocates nothing.
   */
  step(tSec: number, nowMs: number, rand: () => number = Math.random): void {
    const { level, peak, peakUntil, clipUntil, act, ph } = this;
    for (let i = 0; i < this.ids.length; i++) {
      const a = act[i];
      let target = -90;
      if (a > ACTIVE_THRESHOLD) {
        target = -42 + a * 40 + Math.sin(tSec * (1.3 + a * 2) + ph[i]) * 6 + (rand() * 10 - 7);
        if (rand() < 0.06) target += 6;
      }
      const cur = level[i];
      level[i] = target > cur ? cur + (target - cur) * 0.7 : Math.max(-90, cur - 1.4);
      if (level[i] > -0.3) clipUntil[i] = nowMs + CLIP_HOLD_MS;
      if (!peakUntil[i] || level[i] >= peak[i] || nowMs > peakUntil[i]) {
        peak[i] = level[i];
        peakUntil[i] = nowMs + PEAK_HOLD_MS;
      }
    }
  }

  /**
   * Frozen mode for fixture screenshots: every slot snaps to a fixed,
   * deterministic value derived from its activity (the design's target mean
   * at t=0 with the random term at its expectation), peaks follow, clips
   * clear. Repeated calls return identical values; no randomness consumed.
   */
  freeze(): void {
    for (let i = 0; i < this.ids.length; i++) {
      const a = this.act[i];
      const v = a > ACTIVE_THRESHOLD ? -42 + a * 40 + Math.sin(this.ph[i]) * 6 - 2 : -90;
      this.level[i] = v;
      this.peak[i] = v;
      this.peakUntil[i] = Number.POSITIVE_INFINITY;
      this.clipUntil[i] = 0;
    }
  }

  levelOf(id: string): number {
    const i = this.slot(id);
    return i < 0 ? -90 : this.level[i];
  }

  peakOf(id: string): number {
    const i = this.slot(id);
    return i < 0 ? -90 : this.peak[i];
  }

  clipping(id: string, nowMs: number): boolean {
    const i = this.slot(id);
    return i >= 0 && this.clipUntil[i] > nowMs;
  }

  /** Meter cover fraction 0..1 for a strip meter (mirrors dbToPos mapping use). */
  static coverOf(levelDb: number, dbToPos: (db: number) => number): number {
    return clamp(1 - dbToPos(levelDb), 0, 1);
  }
}

/**
 * The id set the design steps each tick: current bank items plus the selected
 * channel, the master, the SOF target mix and the odd FX returns. Port of the
 * ids array built at the top of stepMeters.
 */
export function meterIdsFor(bankItemIds: string[], sel: string, mix: number): string[] {
  const ids = bankItemIds.slice();
  const extra = [sel, 'mainlr', 'mix' + (mix + 1), 'fx1', 'fx3', 'fx5', 'fx7', 'fx9', 'fx11', 'fx13', 'fx15'];
  for (const x of extra) {
    if (ids.indexOf(x) < 0) ids.push(x);
  }
  return ids;
}

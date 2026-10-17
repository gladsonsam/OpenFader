// Sends-on-fader send-level cache.
//
// The backend `state` frames carry only fader/mute/name — no send levels —
// so per-(channel,bus) send levels live here (persisted) and are pushed
// through the EXISTING `param` command as `auxN_level` dB. No protocol
// change: a relay that rejects the field degrades to a local rehearsal
// cache, same as other unsupported params.

import { loadJSON, saveJSON, LS } from './persist';

export const BUS_BASE = 32; // sim geometry: ch 0..31 inputs, 32..39 buses
export const N_SOF = 8; // aux-mapped buses

/** Canonical bus test: sim geometry first, name fallback for narrow backends. */
export const isBusCh = (ch: number): boolean => ch >= BUS_BASE;

/** 0-based aux index for a bus channel, or -1 when unmappable. */
export const busIdx = (busCh: number): number => {
  const i = busCh - BUS_BASE;
  return i >= 0 && i < N_SOF ? i : -1;
};

/** Detail-panel field carrying this bus's send level (aux1..aux8_level). */
export const auxField = (busCh: number): string => `aux${busIdx(busCh) + 1}_level`;

const BUS_COLORS = ['#f5b83f', '#46c46a', '#3fa9f5', '#b07ff0', '#f07fc8', '#ff8a5c', '#5cd6d6', '#e5484d'];

/** Yamaha-style color ownership: one color per destination bus. */
export const busColor = (busCh: number): string => {
  const i = busIdx(busCh);
  return BUS_COLORS[((i % BUS_COLORS.length) + BUS_COLORS.length) % BUS_COLORS.length] ?? '#f5b83f';
};

export const clamp01 = (v: number): number => Math.min(1, Math.max(0, v));

/** Send dB (-120..+10, matches sections.ts aux ranges) <-> fader 0..1. */
export const dbToFader = (db: number): number => clamp01((db + 120) / 130);
export const faderToDb = (v: number): number => -120 + clamp01(v) * 130;

export function fmtSendDb(db: number): string {
  if (db <= -119.5) return '-inf';
  return `${db >= 0 ? '+' : ''}${db.toFixed(1)}`;
}

export type SendMap = Record<string, number>;

export const sendKey = (ch: number, bus: number): string => `${ch}>${bus}`;

export function loadSends(): SendMap {
  return loadJSON<SendMap>(LS.sends, {});
}

export function saveSends(m: SendMap): void {
  saveJSON(LS.sends, m);
}

/** Send level in dB for (channel -> bus); virgin sends start at -inf. */
export function sendDb(m: SendMap, ch: number, bus: number): number {
  const v = m[sendKey(ch, bus)];
  return typeof v === 'number' && Number.isFinite(v) ? v : -120;
}

// Per-strip EQ band store: ch -> live EqBand[] for the mixer thumbs.
//
// Feed contract (see Strip.svelte / FatStrip.svelte headers): App passes
// `bands={eqBandsFor(ch)} grDb={grFor(ch, gr)}` at each <Strip> call site.
// FatStrip renders the same curve math as EqGraph from that copy, so mixer
// strips show the channel's EQ without opening Channel view.
//
// Seeding: backend `state` frames carry ch/name/fader/muted only — no EQ —
// so each channel seeds from EQ_DEFAULTS (engineering-unit defs, same as
// Detail) on first read.
//
// Updates: every Detail/EqGraph edit flows through App.pushParam, which
// calls noteBandParam(ch, field, value) optimistically. There is NO backend
// param-echo path to subscribe to: the relay only sends
// state/meters/spectrum/gr/shows frames (see backend.ts WsBackend.handle —
// no `{type:'param'}` message), and the Tauri shell has no param query
// either. So this store is a local-edit mirror: exact numbers still reach
// the backend via the existing `param` cmd, and the thumbs stay indicative
// (a backend that rejects a field hides that control; the thumb keeps the
// last local intent).
//
// Reactivity: plain .ts (no runes), so state lives in a SvelteMap —
// `eqBandsFor` reads track per-channel in render, and noteBandParam replaces
// the channel array via .set so thumbs invalidate on edit.

import { SvelteMap } from 'svelte/reactivity';
import { EQ_DEFAULTS, clampFreq, clampGain, clampQ, type EqBand } from './eqCurve';
import type { ParamValue } from './backend';

const store = new SvelteMap<number, EqBand[]>();

/** Live band state for ch (MAIN ch<0: flat defaults, caller passes null). */
export function eqBandsFor(ch: number): EqBand[] {
  let bands = store.get(ch);
  if (!bands) {
    bands = EQ_DEFAULTS.map((b) => ({ ...b }));
    store.set(ch, bands);
  }
  return bands;
}

/** Forget a channel's copy (next read re-seeds from defaults). */
export function resetBandsFor(ch: number): void {
  store.delete(ch);
}

const EQ_PARAM = /^eq([1-4])_(gain|freq|q|type)$/;

/**
 * Mirror one user edit into the store. Non-EQ fields and non-finite values
 * are ignored; the backend still gets the exact original via `param`.
 */
export function noteBandParam(ch: number, field: string, value: ParamValue): void {
  const m = EQ_PARAM.exec(field);
  if (!m || typeof value !== 'number' || !Number.isFinite(value)) return;
  const n = Number(m[1]) - 1;
  const kind = m[2];
  const bands = eqBandsFor(ch);
  const cur = bands[n];
  if (!cur) return;
  const next: EqBand = { ...cur };
  if (kind === 'gain') next.gain = clampGain(value);
  else if (kind === 'freq') next.freq = clampFreq(value);
  else if (kind === 'q') next.q = clampQ(value);
  else next.type = Math.trunc(value); // type word: exact echo, no convention clamp
  store.set(ch, bands.map((b, i) => (i === n ? next : b)));
}

/** Live comp GR dB for a strip thumb (null = no GR feed yet). */
export function grFor(ch: number, gr: Record<number, { gate: number; comp: number }>): number | null {
  return gr[ch]?.comp ?? null;
}

// D2 Data package: exact TypeScript port of the design's seeded data model.
// Source: docs/design/src/design-logic.js.txt (class Component: bankDefs,
// buildChannels with seeded PRNG seed=7, buildFx, scenes, devices) plus the
// fader-law / formatting / EQ / dynamics math the template reads.
// The PRNG consumption ORDER in buildChannels() is load-bearing: it must stay
// identical to the design or every value drifts. Do not reorder calls to r().

export type ChannelKind = 'in' | 'ret' | 'bus' | 'out' | 'dca';
export type EqBandType = 'LS' | 'PEQ' | 'HS';

export interface BankItem {
  id: string;
  name: string;
  src: string;
  eq: boolean;
  kind: ChannelKind;
}

export interface Bank {
  key: string;
  label: string;
  color: string;
  text: string;
  items: BankItem[];
}

export interface Hpf {
  on: boolean;
  f: number;
  slope: 12 | 24;
}

export interface EqBand {
  type: EqBandType;
  f: number;
  g: number;
  q: number;
  on: boolean;
}

export interface Eq {
  on: boolean;
  hpf: Hpf;
  bands: EqBand[]; // [Low, Lo-Mid, Hi-Mid, High]
}

export interface Gate {
  on: boolean;
  thr: number;
  range: number;
  att: number;
  hold: number;
  rel: number;
}

export interface Comp {
  on: boolean;
  thr: number;
  ratio: number;
  att: number;
  rel: number;
  knee: number;
  makeup: number;
}

export interface Dyn {
  gate: Gate;
  comp: Comp;
}

export interface ChannelInput {
  gain: number;
  trim: number;
  phantom: boolean;
  invert: boolean;
  delayOn: boolean;
  delay: number;
}

export interface Channel {
  id: string;
  name: string;
  src: string;
  color: string;
  text: string;
  /** Channel strip icon (UI-only, no desk counterpart). Null = none. */
  icon?: string | null;
  kind: ChannelKind;
  act: number;
  ph: number;
  fader: number;
  mute: boolean;
  /** Solo (UI-only; no desk/relay counterpart). */
  solo: boolean;
  pan: number | null;
  sends: number[] | null;
  sendPre: boolean[] | null;
  input: ChannelInput | null;
  eq: Eq | null;
  dyn: Dyn | null;
}

export interface FxSlot {
  type: string;
  on: boolean;
  p: number[];
  bpm: number | null;
}

export interface Device {
  key: string;
  name: string;
  model: string;
  ip: string;
  kind: 'console' | 'io';
}

// ---------- banks (port of bankDefs) ----------

const BANK_GREEN = '#5cdb5c';
const BANK_GREEN_TEXT = '#10240f';

/**
 * Console-tape palette for input channels + FX returns: one colour per
 * channel position (mod 8), so a bank reads as grouped tape strips instead
 * of a single block of green. Buses, groups, matrix and DCAs keep their
 * family colours (functional meaning). Every entry pairs a muted tone with
 * a contrast-safe label colour for the name bars.
 */
export const CHAN_TAPE: { color: string; text: string }[] = [
  { color: '#d64545', text: '#ffffff' }, // red
  { color: '#e8853d', text: '#331503' }, // orange
  { color: '#e3c341', text: '#2e2503' }, // yellow
  { color: '#5cdb5c', text: '#10240f' }, // green (bank colour)
  { color: '#22d3e6', text: '#062a30' }, // cyan
  { color: '#4f8ff7', text: '#ffffff' }, // blue
  { color: '#8e4ec6', text: '#ffffff' }, // purple
  { color: '#e93d82', text: '#ffffff' }, // pink
];

function range<T>(n: number, f: (i: number) => T): T[] {
  return Array.from({ length: n }, (_, i) => f(i));
}

const pad2 = (n: number): string => String(n).padStart(2, '0');

let banksCache: Bank[] | null = null;

export function bankDefs(): Bank[] {
  if (banksCache) return banksCache;
  banksCache = [
    { key: 'ch1', label: 'Ch 1-16', color: BANK_GREEN, text: BANK_GREEN_TEXT, items: range(16, i => ({ id: 'in' + (i + 1), name: 'Ch ' + (i + 1), src: 'Lcl ' + pad2(i + 1), eq: true, kind: 'in' as ChannelKind })) },
    { key: 'ch17', label: 'Ch 17-32', color: BANK_GREEN, text: BANK_GREEN_TEXT, items: range(16, i => ({ id: 'in' + (i + 17), name: 'Ch ' + (i + 17), src: 'Lcl ' + pad2(i + 17), eq: true, kind: 'in' as ChannelKind })) },
    { key: 'ch33', label: 'Ch 33-48', color: BANK_GREEN, text: BANK_GREEN_TEXT, items: range(16, i => ({ id: 'in' + (i + 33), name: 'Ch ' + (i + 33), src: 'Net ' + pad2(i + 1), eq: true, kind: 'in' as ChannelKind })) },
    { key: 'fx', label: 'FxRet', color: BANK_GREEN, text: BANK_GREEN_TEXT, items: range(16, i => ({ id: 'fx' + (i + 1), name: 'FxRet ' + (i + 1), src: 'FX ' + (Math.floor(i / 2) + 1) + (i % 2 ? 'R' : 'L'), eq: true, kind: 'ret' as ChannelKind })) },
    { key: 'mix', label: 'Mix', color: '#22d3e6', text: '#062a30', items: range(16, i => ({ id: 'mix' + (i + 1), name: 'Mix ' + (i + 1), src: 'Bus', eq: true, kind: 'bus' as ChannelKind })) },
    { key: 'grp', label: 'Grp/FxSnd', color: '#2563eb', text: '#ffffff', items: range(8, i => ({ id: 'grp' + (i + 1), name: 'Grp ' + (i + 1), src: 'Bus', eq: true, kind: 'bus' as ChannelKind })).concat(range(8, i => ({ id: 'fxs' + (i + 1), name: 'FxSnd ' + (i + 1), src: 'FX ' + (i + 1), eq: true, kind: 'bus' as ChannelKind }))) },
    { key: 'mtx', label: 'Mtx/Main', color: '#c026d3', text: '#ffffff', items: range(8, i => ({ id: 'mtx' + (i + 1), name: 'Mtx ' + (i + 1), src: 'Out', eq: true, kind: 'out' as ChannelKind })).concat([{ id: 'mainlr', name: 'Main LR', src: 'Out', eq: true, kind: 'out' as ChannelKind }, { id: 'mainc', name: 'Main C', src: 'Out', eq: true, kind: 'out' as ChannelKind }]) },
    { key: 'dca', label: 'DCA', color: '#dc2626', text: '#ffffff', items: range(16, i => ({ id: 'dca' + (i + 1), name: 'DCA ' + (i + 1), src: 'DCA', eq: false, kind: 'dca' as ChannelKind })) },
  ];
  return banksCache;
}

export function bankByKey(key: string): Bank {
  const b = bankDefs().find(b => b.key === key);
  if (!b) throw new Error('unknown bank: ' + key);
  return b;
}

// ---------- channels (port of buildChannels; r() order is binding) ----------

const half = (v: number): number => Math.round(v * 2) / 2;

export function buildChannels(): Record<string, Channel> {
  let seed = 7;
  const r = (): number => { seed = (seed * 16807) % 2147483647; return (seed - 1) / 2147483646; };
  const out: Record<string, Channel> = {};
  bankDefs().forEach(b => b.items.forEach((it, idx) => {
    const isIn = it.kind === 'in' || it.kind === 'ret';
    const tape = isIn ? CHAN_TAPE[idx % CHAN_TAPE.length]! : null;
    out[it.id] = {
      id: it.id, name: it.name, src: it.src, color: tape?.color ?? b.color, text: tape?.text ?? b.text, kind: it.kind,
      act: it.kind === 'dca' ? 0.55 : (r() < 0.15 ? 0.05 : 0.35 + r() * 0.62),
      ph: r() * 6,
      fader: it.kind === 'dca' ? 0.75 : 0.6 + r() * 0.15,
      mute: false,
      solo: false,
      pan: (it.kind === 'dca' || it.kind === 'out') ? null : (r() < 0.5 ? 0 : Math.round((r() * 2 - 1) * 60) / 100),
      sends: isIn ? Array.from({ length: 16 }, () => Math.round(r() * 70) / 100) : null,
      sendPre: isIn ? Array.from({ length: 16 }, (_, k) => k < 8) : null,
      input: it.kind === 'in' ? { gain: Math.round(20 + r() * 30), trim: 0, phantom: false, invert: false, delayOn: false, delay: 0 } : null,
      eq: it.eq ? {
        on: true,
        hpf: { on: it.kind === 'in' && r() < 0.7, f: 60 + Math.round(r() * 80), slope: 12 },
        bands: [
          { type: 'LS', f: 120, g: r() < 0.4 ? half(r() * 8 - 4) : 0, q: 0.7, on: true },
          { type: 'PEQ', f: Math.round(250 + r() * 400), g: r() < 0.5 ? -half(r() * 6) : 0, q: 1.4, on: true },
          { type: 'PEQ', f: Math.round(1800 + r() * 2500), g: r() < 0.5 ? half(r() * 5) : 0, q: Math.round((1.2 + r()) * 100) / 100, on: true },
          { type: 'HS', f: 8000, g: r() < 0.4 ? half(r() * 4) : 0, q: 0.7, on: true },
        ],
      } : null,
      dyn: it.eq ? {
        gate: { on: r() < 0.3, thr: Math.round(-55 + r() * 15), range: 40, att: 0.5, hold: 50, rel: 150 },
        comp: { on: r() < 0.7, thr: Math.round(-28 + r() * 16), ratio: [2, 3, 4, 6][Math.floor(r() * 4)], att: 10, rel: 120, knee: 3, makeup: 0 },
      } : null,
    };
  }));
  // Live-desk aux input (XR18 stereo aux return, `/rtn/aux`): a fixture
  // channel outside every fixture bank (no PRNG draws — the seeded values
  // above must stay identical). Offline it never renders (no bank lists
  // it); online on xair it joins the default mixer view after Ch 16.
  // Fixed values, never random: the live sync overwrites them on connect.
  out['aux1'] = {
    id: 'aux1', name: 'Aux', src: 'Aux In', color: '#5cdb5c', text: '#10240f', kind: 'ret',
    act: 0.35, ph: 0, fader: 0.6, mute: false, solo: false, pan: 0,
    sends: null, sendPre: null, input: null,
    eq: {
      on: true,
      hpf: { on: false, f: 80, slope: 12 },
      bands: [
        { type: 'LS', f: 120, g: 0, q: 0.7, on: true },
        { type: 'PEQ', f: 800, g: 0, q: 1.4, on: true },
        { type: 'PEQ', f: 3000, g: 0, q: 1.4, on: true },
        { type: 'HS', f: 8000, g: 0, q: 0.7, on: true },
      ],
    },
    dyn: null,
  };
  return out;
}

// ---------- fx / scenes / devices (ports of buildFx, scenes, devices) ----------

export const FX_TYPES = ['Hall', 'Plate', 'Room', 'Delay', 'Chorus', 'Flanger'] as const;

export function buildFx(): FxSlot[] {
  const T = ['Hall', 'Plate', 'Delay', 'Chorus', 'Room', 'Delay', 'Flanger', 'Hall'];
  const P: Record<string, number[]> = {
    Hall: [0.42, 0.15, 0.6, 0.55, 1],
    Plate: [0.25, 0.08, 0.5, 0.65, 1],
    Room: [0.1, 0.05, 0.3, 0.6, 1],
    Delay: [0.55, 0.35, 0.55, 0.4, 1],
    Chorus: [0.4, 0.5, 0.6, 1, 1],
    Flanger: [0.3, 0.6, 0.4, 1, 1],
  };
  return T.map((t, i) => ({ type: t, on: i !== 7, p: P[t].slice(), bpm: null }));
}

export function fxRetLabel(slotIndex: number): string {
  return 'Ret ' + (2 * slotIndex + 1) + '/' + (2 * slotIndex + 2);
}

export function fxStateLabel(slot: FxSlot): string {
  return slot.on ? 'On' : 'Bypassed';
}

export function scenes(): string[] {
  return ['Soundcheck', 'Doors', 'Walk-in', 'Support act', 'Changeover', 'Set 1', 'Acoustic', 'Set 2', 'Encore', 'Walk-out'];
}

export function devices(): Device[] {
  return [
    { key: 'foh', name: 'FOH Console', model: '[Console model]', ip: '192.168.1.120', kind: 'console' },
    { key: 'mon', name: 'Monitor Console', model: '[Console model]', ip: '192.168.1.21', kind: 'console' },
    { key: 'sb1', name: 'Stage Box A', model: '[I/O model]', ip: '192.168.1.40', kind: 'io' },
  ];
}

// ---------- fader law + formatting (ports of law/posToDb/dbToPos/fmt*) ----------

export type LawPoint = [number, number];

export function law(): LawPoint[] {
  return [[0, -90], [0.125, -50], [0.25, -30], [0.375, -20], [0.5, -10], [0.625, -5], [0.75, 0], [0.875, 5], [1, 10]];
}

export function posToDb(p: number): number {
  if (p <= 0.003) return -Infinity;
  const T = law();
  for (let i = 1; i < T.length; i++) {
    if (p <= T[i][0]) {
      const a = T[i - 1], b = T[i];
      return a[1] + (b[1] - a[1]) * (p - a[0]) / (b[0] - a[0]);
    }
  }
  return 10;
}

export function dbToPos(db: number): number {
  const T = law();
  if (db <= -90) return 0;
  for (let i = 1; i < T.length; i++) {
    if (db <= T[i][1]) {
      const a = T[i - 1], b = T[i];
      return a[0] + (b[0] - a[0]) * (db - a[1]) / (b[1] - a[1]);
    }
  }
  return 1;
}

/** dB readout with one decimal, e.g. `-4.3`, `+2.0`; `-∞` below -89. */
export function fmtDb(p: number): string {
  const d = posToDb(p);
  if (!isFinite(d) || d < -89) return '-∞';
  return (d > 0.04 ? '+' : '') + d.toFixed(1);
}

export function fmtF(f: number): string {
  return f < 1000 ? Math.round(f) + ' Hz' : (f / 1000).toFixed(f < 10000 ? 2 : 1) + ' kHz';
}

export function fmtG(g: number): string {
  return (g > 0 ? '+' : '') + g.toFixed(1) + ' dB';
}

export function fmtMs(v: number): string {
  return v >= 1000 ? (v / 1000).toFixed(2) + ' s' : v < 10 ? v.toFixed(2) + ' ms' : v < 100 ? v.toFixed(1) + ' ms' : Math.round(v) + ' ms';
}

export function clamp(v: number, a: number, b: number): number {
  return Math.max(a, Math.min(b, v));
}

/** Pan readout: `C`, `L28`, `R45`, or '' when the strip has no pan. */
export function panText(pan: number | null): string {
  if (pan == null) return '';
  return pan === 0 ? 'C' : (pan < 0 ? 'L' : 'R') + Math.round(Math.abs(pan) * 100);
}

/** Strip aria labels, exactly as the design builds them in stripVals. */
export function panAria(name: string, pan: number | null): string {
  return 'Pan ' + name + ' ' + panText(pan);
}

export function faderAria(name: string, pos: number, useSend: boolean, mix: number): string {
  return (useSend ? 'Send to Mix ' + (mix + 1) + ' from ' : 'Fader ') + name + ' ' + fmtDb(pos) + ' dB';
}

export function sendAria(mixIndex: number, v: number): string {
  return 'Send to Mix ' + (mixIndex + 1) + ' ' + fmtDb(v) + ' dB';
}

/** Ratio readout, e.g. `3.0:1`. */
export function fmtRatio(ratio: number): string {
  return ratio.toFixed(1) + ':1';
}

// ---------- EQ math (RBJ biquads @ 48k; ports of coefs/magDb/filters/resp) ----------

export type Biquad = [number, number, number, number, number];

export function coefs(type: string, f: number, g: number, q: number): Biquad {
  const fs = 48000, A = Math.pow(10, g / 40), w = 2 * Math.PI * f / fs;
  const cs = Math.cos(w), sn = Math.sin(w), al = sn / (2 * q);
  let b0: number, b1: number, b2: number, a0: number, a1: number, a2: number;
  if (type === 'PEQ') { b0 = 1 + al * A; b1 = -2 * cs; b2 = 1 - al * A; a0 = 1 + al / A; a1 = -2 * cs; a2 = 1 - al / A; }
  else if (type === 'LS') { const s = 2 * Math.sqrt(A) * al; b0 = A * ((A + 1) - (A - 1) * cs + s); b1 = 2 * A * ((A - 1) - (A + 1) * cs); b2 = A * ((A + 1) - (A - 1) * cs - s); a0 = (A + 1) + (A - 1) * cs + s; a1 = -2 * ((A - 1) + (A + 1) * cs); a2 = (A + 1) + (A - 1) * cs - s; }
  else if (type === 'HS') { const s = 2 * Math.sqrt(A) * al; b0 = A * ((A + 1) + (A - 1) * cs + s); b1 = -2 * A * ((A - 1) + (A + 1) * cs); b2 = A * ((A + 1) + (A - 1) * cs - s); a0 = (A + 1) - (A - 1) * cs + s; a1 = 2 * ((A - 1) - (A + 1) * cs); a2 = (A + 1) - (A - 1) * cs - s; }
  else { b0 = (1 + cs) / 2; b1 = -(1 + cs); b2 = (1 + cs) / 2; a0 = 1 + al; a1 = -2 * cs; a2 = 1 - al; }
  return [b0 / a0, b1 / a0, b2 / a0, a1 / a0, a2 / a0];
}

export function magDb(c: Biquad, f: number): number {
  const w = 2 * Math.PI * f / 48000;
  const c1 = Math.cos(w), s1 = Math.sin(w), c2 = Math.cos(2 * w), s2 = Math.sin(2 * w);
  const nr = c[0] + c[1] * c1 + c[2] * c2, ni = -(c[1] * s1 + c[2] * s2);
  const dr = 1 + c[3] * c1 + c[4] * c2, di = -(c[3] * s1 + c[4] * s2);
  return 10 * Math.log10((nr * nr + ni * ni) / (dr * dr + di * di));
}

export function filters(eq: Eq | null): Biquad[] {
  const out: Biquad[] = [];
  if (!eq || !eq.on) return out;
  if (eq.hpf && eq.hpf.on) {
    const c = coefs('HP', eq.hpf.f, 0, 0.7071);
    out.push(c);
    if (eq.hpf.slope === 24) out.push(c);
  }
  (eq.bands || []).forEach(b => { if (b.on) out.push(coefs(b.type, b.f, b.g, b.q)); });
  return out;
}

export function resp(fl: Biquad[], f: number): number {
  let s = 0;
  for (let i = 0; i < fl.length; i++) s += magDb(fl[i], f);
  return s;
}

/** Strip EQ thumbnail path (40 segments over 20 Hz..20 kHz). Pure; no cache. */
export function eqThumbPath(ch: Channel): string {
  if (!ch.eq) return '';
  const fl = filters(ch.eq);
  const pts: string[] = [];
  for (let i = 0; i <= 40; i++) {
    const f = 20 * Math.pow(1000, i / 40);
    const db = fl.length ? resp(fl, f) : 0;
    pts.push((i * 2.5).toFixed(1) + ' ' + Math.max(0, Math.min(40, 20 - db / 15 * 20)).toFixed(1));
  }
  return 'M' + pts.join(' L');
}

// ---------- dynamics math (ports of gateOut/compOut/dynThumb) ----------

export function gateOut(g: Gate, x: number): number {
  return g.on && x < g.thr ? x - g.range : x;
}

export function compOut(c: Comp, x: number): number {
  if (!c.on) return x;
  const d = x - c.thr, k = c.knee;
  let y: number;
  if (2 * d < -k) y = x;
  else if (k > 0 && 2 * Math.abs(d) <= k) y = x + (1 / c.ratio - 1) * Math.pow(d + k / 2, 2) / (2 * k);
  else y = c.thr + d / c.ratio;
  return y + c.makeup;
}

export function dynThumbPath(ch: Channel): string {
  if (!ch.dyn) return '';
  const pts: string[] = [];
  for (let i = 0; i <= 24; i++) {
    const inp = -60 + i * 2.5;
    const o = compOut(ch.dyn.comp, gateOut(ch.dyn.gate, inp));
    pts.push(((inp + 60) / 60 * 100).toFixed(1) + ' ' + Math.max(0, Math.min(40, -o / 60 * 40)).toFixed(1));
  }
  return 'M' + pts.join(' L');
}

// ---------- knob specs (port of spec/toN/fromN) ----------

export interface KnobSpec {
  min: number;
  max: number;
  step?: number;
  log?: boolean;
  fmt: (v: number) => string;
}

const specDb = (v: number): string => v.toFixed(1) + ' dB';

export function spec(key: string): KnobSpec {
  const ms = (v: number): string => fmtMs(v);
  const table: Record<string, KnobSpec> = {
    gain: { min: 0, max: 60, step: 0.5, fmt: specDb },
    trim: { min: -18, max: 18, step: 0.5, fmt: (v) => fmtG(v) },
    delay: { min: 0, max: 500, step: 0.1, fmt: ms },
    gthr: { min: -80, max: 0, step: 0.5, fmt: specDb },
    range: { min: 0, max: 80, step: 1, fmt: (v) => v + ' dB' },
    gatt: { min: 0.05, max: 50, log: true, fmt: ms },
    hold: { min: 1, max: 2000, log: true, fmt: ms },
    grel: { min: 5, max: 4000, log: true, fmt: ms },
    cthr: { min: -60, max: 0, step: 0.5, fmt: specDb },
    ratio: { min: 1, max: 20, log: true, fmt: (v) => v.toFixed(1) + ':1' },
    catt: { min: 0.1, max: 100, log: true, fmt: ms },
    crel: { min: 5, max: 2000, log: true, fmt: ms },
    knee: { min: 0, max: 12, step: 0.5, fmt: specDb },
    makeup: { min: 0, max: 24, step: 0.5, fmt: (v) => fmtG(v) },
  };
  const s = table[key];
  if (!s) throw new Error('unknown spec: ' + key);
  return s;
}

export function toN(s: KnobSpec, v: number): number {
  return s.log ? Math.log(v / s.min) / Math.log(s.max / s.min) : (v - s.min) / (s.max - s.min);
}

export function fromN(s: KnobSpec, n: number): number {
  const v = s.log ? s.min * Math.pow(s.max / s.min, n) : s.min + n * (s.max - s.min);
  return s.step ? Math.round(v / s.step) * s.step : Math.round(v * 100) / 100;
}

// ---------- knob ring geometry (port of knobItem dash/rot) ----------

export function knobDash(n: number): string {
  return (clamp(n, 0, 1) * 103.67).toFixed(2) + ' 138.23';
}

export function knobRot(n: number): string {
  return 'rotate(' + (-135 + clamp(n, 0, 1) * 270).toFixed(1) + ' 28 28)';
}

/** EQ-graph x normalisation (port of xOf in eqVals). */
export function eqXOf(f: number): number {
  return Math.log(f / 20) / Math.log(1000);
}

// ---------- display-text helpers (what each screen reads) ----------

export interface EqBandTexts {
  freq: string;
  gain: string;
  q: string;
}

export function eqBandTexts(band: EqBand): EqBandTexts {
  return { freq: fmtF(band.f), gain: fmtG(band.g), q: band.q.toFixed(2) };
}

export function hpfTexts(hpf: Hpf): { freq: string; slope: string } {
  return { freq: fmtF(hpf.f), slope: hpf.slope + ' dB/oct' };
}

export interface DynKnobTexts {
  label: string;
  value: string;
}

function dynKnob(key: string, label: string, value: number): DynKnobTexts {
  return { label, value: spec(key).fmt(value) };
}

export function gateKnobTexts(g: Gate): DynKnobTexts[] {
  return [
    dynKnob('gthr', 'Threshold', g.thr),
    dynKnob('range', 'Range', g.range),
    dynKnob('gatt', 'Attack', g.att),
    dynKnob('hold', 'Hold', g.hold),
    dynKnob('grel', 'Release', g.rel),
  ];
}

export function compKnobTexts(c: Comp): DynKnobTexts[] {
  return [
    dynKnob('cthr', 'Threshold', c.thr),
    dynKnob('ratio', 'Ratio', c.ratio),
    dynKnob('knee', 'Knee', c.knee),
    dynKnob('catt', 'Attack', c.att),
    dynKnob('crel', 'Release', c.rel),
    dynKnob('makeup', 'Makeup', c.makeup),
  ];
}

export function inputKnobTexts(input: ChannelInput): DynKnobTexts[] {
  return [
    dynKnob('gain', 'Gain', input.gain),
    dynKnob('trim', 'Trim', input.trim),
  ];
}

export function delayKnobTexts(input: ChannelInput): DynKnobTexts[] {
  return [dynKnob('delay', 'Time', input.delay)];
}

export function delayDistanceText(delayMs: number): string {
  return '≈ ' + (delayMs * 0.343).toFixed(2) + ' m of distance';
}

/** Compressor summary line on the dyn tab, e.g. `Thr -13.0 dB · 3.0:1 · Makeup +0.0 dB`. */
export function compSummary(c: Comp): string {
  return 'Thr ' + c.thr.toFixed(1) + ' dB · ' + c.ratio.toFixed(1) + ':1 · Makeup ' + fmtG(c.makeup);
}

export function gateSummary(g: Gate): string {
  return 'Thr ' + g.thr.toFixed(1) + ' dB · Range ' + g.range + ' dB';
}

/** FX knob value texts for a slot, mirroring fxDefs formatters. */
export function fxKnobDefs(fxType: string): Array<{ label: string; fmt: (v: number) => string }> {
  const pct = (v: number): string => Math.round(v * 100) + '%';
  const hz = (v: number): string => fmtF(1000 * Math.pow(20, v));
  const rev = [
    { label: 'Decay', fmt: (v: number): string => (0.2 + v * 9.8).toFixed(1) + ' s' },
    { label: 'Pre-delay', fmt: (v: number): string => Math.round(v * 200) + ' ms' },
    { label: 'Size', fmt: pct },
    { label: 'Hi Damp', fmt: hz },
    { label: 'Mix', fmt: pct },
  ];
  const rate = { label: 'Rate', fmt: (v: number): string => (0.05 * Math.pow(100, v)).toFixed(2) + ' Hz' };
  const table: Record<string, Array<{ label: string; fmt: (v: number) => string }>> = {
    Hall: rev,
    Plate: rev,
    Room: rev,
    Delay: [
      { label: 'Time', fmt: (v) => fmtMs(20 * Math.pow(100, v)) },
      { label: 'Feedback', fmt: (v) => Math.round(v * 95) + '%' },
      { label: 'Hi Cut', fmt: hz },
      { label: 'Spread', fmt: pct },
      { label: 'Mix', fmt: pct },
    ],
    Chorus: [rate, { label: 'Depth', fmt: pct }, { label: 'Spread', fmt: pct }, { label: 'Mix', fmt: pct }],
    Flanger: [rate, { label: 'Depth', fmt: pct }, { label: 'Feedback', fmt: (v) => Math.round(v * 90) + '%' }, { label: 'Mix', fmt: pct }],
  };
  const defs = table[fxType];
  if (!defs) throw new Error('unknown fx type: ' + fxType);
  return defs;
}

export function fxKnobTexts(slot: FxSlot): DynKnobTexts[] {
  const defs = fxKnobDefs(slot.type);
  return defs.map((d, i) => ({ label: d.label, value: d.fmt(slot.p[i]) }));
}

/** Tap-tempo math (port of tap): feeds the previous tap times + now, returns the
 *  updated tap list and, once >= 2 taps exist, the bpm and normalised p[0]. */
export function tapUpdate(prevTaps: number[], now: number): { taps: number[]; bpm: number | null; p0: number | null } {
  const taps = prevTaps.filter(t => now - t < 2500);
  taps.push(now);
  if (taps.length < 2) return { taps, bpm: null, p0: null };
  const iv = (taps[taps.length - 1] - taps[0]) / (taps.length - 1);
  const bpm = Math.round(60000 / iv);
  const ms = clamp(60000 / bpm, 20, 2000);
  return { taps, bpm, p0: Math.log(ms / 20) / Math.log(100) };
}

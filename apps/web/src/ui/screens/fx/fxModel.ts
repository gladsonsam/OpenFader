// F1 FX screen: pure view-model builder (rune-free, DOM-free, node-testable).
//
// Exact port of the `fxVals` branch of the design's renderVals
// (docs/design/src/design-logic.js.txt + docs/design/src/design-template.html
// lines 506-554) over the fx math in fixtures/designData.ts (fxDefs port:
// fxKnobDefs/fxKnobTexts, fxRetLabel, fxStateLabel, tapUpdate).
// Components stay presentational: FxScreen binds this to uiStore state + fx
// and wires ui.do actions.
//
// Design facts encoded:
//   slot grid: repeat(auto-fit, minmax(140px, 1fr)), gap 6;
//   slot card: button, h 96, border 2px (sel #b28cff else var(--u4-line-faint)),
//     bg (sel #1b2330 else var(--u4-panel-bg)); name 12px var(--u4-text-5); state 11px/600
//     (on #7be27b else #f0b44a); type 18px/600; ret 12px var(--u4-text-7);
//     meter bar h 4 bg #1a2332, fill #b28cff, width on ? dbToPos% : 0;
//   detail panel: flex 1 1 auto, min-h 300, col, gap 16, padding 14,
//     radius 3, border var(--u4-line-panel), bg var(--u4-panel-bg);
//   title 20px/600 `FX n · Type`; tap button (Delay only) h 44
//     `Tap · N BPM` | `Tap tempo`; on toggle h 44 (on #5cdb5c/#10240f +
//     border #5cdb5c else #2a1f0a/#f6c76a + border #f0b44a);
//   type grid: auto-fit minmax(96px, 1fr), gap 6, buttons h 44
//     (sel #b28cff/#1a0f2e else var(--u4-btn-bg)/var(--u4-text-4));
//   knobs: flex wrap space-around center gap 16; 92px FX-style ring knobs
//     (P1 Knob, norm mode, accent #b28cff), label 12px var(--u4-text-6),
//     value 15px var(--u4-text-1); Chorus/Flanger have 4 knobs, the rest 5.
//
// Run: node --experimental-strip-types apps/web/src/ui/screens/fx/fxModel.test.ts

import {
  dbToPos,
  fxKnobDefs,
  fxRetLabel,
  fxStateLabel,
  type FxSlot,
} from '../../fixtures/designData.ts';
import { fxTitleText } from '../../state/uiLogic.ts';

export const FX_TYPES = ['Hall', 'Plate', 'Room', 'Delay', 'Chorus', 'Flanger'] as const;
export type FxTypeName = (typeof FX_TYPES)[number];

/** Purple FX accent (design knobItem colour for the FX screen). */
export const FX_ACCENT = '#b28cff';

/** Visual identity for one effect family (clean-room CSS placeholder).
 * There are no shippable per-effect images (the only known faceplates
 * are Behringer-copyrighted mixer control software assets — reference-only, never
 * vendored), so each module gets a family gradient + glyph instead.
 */
export interface FxArt {
  /** Effect family, e.g. 'Reverb'. */
  category: string;
  /** CSS background for the swatch (gradient per family). */
  gradient: string;
  /** Glyph drawn on the swatch (basic geometric shape, not an emoji). */
  glyph: string;
}

const FX_FAMILIES: Record<string, FxArt> = {
  Reverb: { category: 'Reverb', gradient: 'linear-gradient(135deg,#3a2a7a,#7a4fd0)', glyph: '●' },
  Delay: { category: 'Delay', gradient: 'linear-gradient(135deg,#0e4a5c,#1d9e8e)', glyph: '◆' },
  Modulation: { category: 'Modulation', gradient: 'linear-gradient(135deg,#73264f,#d14d9c)', glyph: '▲' },
  EQ: { category: 'EQ', gradient: 'linear-gradient(135deg,#1c4e89,#43a8d8)', glyph: '■' },
  Dynamics: { category: 'Dynamics', gradient: 'linear-gradient(135deg,#7a3c00,#e08a00)', glyph: '▼' },
  Amp: { category: 'Amp', gradient: 'linear-gradient(135deg,#6e1212,#d8492b)', glyph: '◉' },
  Pitch: { category: 'Pitch', gradient: 'linear-gradient(135deg,#0b5e3b,#2ed47f)', glyph: '◎' },
  Enhance: { category: 'Enhance', gradient: 'linear-gradient(135deg,#555500,#c9c93d)', glyph: '◇' },
  Other: { category: 'Other', gradient: 'linear-gradient(135deg,#3a4356,#6b7a94)', glyph: '○' },
};

/** Effect code (desk short code or design name) → family. Desk codes are
 * the `caps.fx` type ids; design names cover the offline fixture rack.
 */
const FX_TYPE_FAMILY: Record<string, string> = {
  HALL: 'Reverb', AMBI: 'Reverb', RPLT: 'Reverb', ROOM: 'Reverb', CHAM: 'Reverb',
  PLAT: 'Reverb', VREV: 'Reverb', VRM: 'Reverb', GATE: 'Reverb', RVRS: 'Reverb',
  DLY: 'Delay', '3TAP': 'Delay', '4TAP': 'Delay', MODD: 'Delay',
  'D/RV': 'Delay', 'D/CR': 'Delay', 'D/FL': 'Delay',
  CRS: 'Modulation', FLNG: 'Modulation', PHAS: 'Modulation', DIMC: 'Modulation',
  FILT: 'Modulation', ROTA: 'Modulation', PAN: 'Modulation',
  'CR/R': 'Modulation', 'FL/R': 'Modulation',
  GEQ2: 'EQ', GEQ: 'EQ', TEQ2: 'EQ', TEQ: 'EQ',
  P1A: 'EQ', P1A2: 'EQ', PQ5: 'EQ', PQ5S: 'EQ',
  DES2: 'Dynamics', DES: 'Dynamics', WAVD: 'Dynamics', LIM: 'Dynamics',
  CMB: 'Dynamics', CMB2: 'Dynamics',
  FAC: 'Dynamics', FAC1M: 'Dynamics', FAC2: 'Dynamics',
  LEC: 'Dynamics', LEC2: 'Dynamics', ULC2: 'Dynamics',
  AMP2: 'Amp', AMP: 'Amp', DRV2: 'Amp', DRV: 'Amp',
  SUB: 'Pitch', PIT2: 'Pitch', PIT: 'Pitch',
  ENH2: 'Enhance', ENH: 'Enhance', EXC2: 'Enhance', EXC: 'Enhance',
  IMG: 'Enhance', EDI: 'Enhance', SON: 'Enhance',
  Hall: 'Reverb', Plate: 'Reverb', Room: 'Reverb',
  Delay: 'Delay', Chorus: 'Modulation', Flanger: 'Modulation',
};

/** Family art for an effect type (desk code or design name); unknown
 * types fall back to the neutral Other tile (never throws — the UI
 * renders honest placeholders for unknown desk state).
 */
export function fxArtFor(type: string): FxArt {
  return FX_FAMILIES[FX_TYPE_FAMILY[type] ?? 'Other']!;
}

/** Number of FX slots on the design's desk (fixture catalogue). */
export const FX_SLOT_COUNT = 8;

/** Descriptor for one knob in a type's knob row (data, not code). */
export interface FxKnobParamDef {
  /** Stable param key, e.g. 'decay'. */
  key: string;
  /** Label shown under the knob, e.g. 'Decay'. */
  label: string;
  /** Catalogue bind suffix, e.g. 'fx:param:decay'. */
  bind: string;
}

/**
 * Knob rows as data (design fxDefs tables): any desk's FX types render
 * without code changes by editing this table. Labels mirror fxKnobDefs in
 * fixtures/designData.ts, which owns the value formatters.
 */
export const FX_PARAM_DEFS: Record<string, FxKnobParamDef[]> = {
  Hall: [
    { key: 'decay', label: 'Decay', bind: 'fx:param:decay' },
    { key: 'predelay', label: 'Pre-delay', bind: 'fx:param:predelay' },
    { key: 'size', label: 'Size', bind: 'fx:param:size' },
    { key: 'hidamp', label: 'Hi Damp', bind: 'fx:param:hidamp' },
    { key: 'mix', label: 'Mix', bind: 'fx:param:mix' },
  ],
  Plate: [
    { key: 'decay', label: 'Decay', bind: 'fx:param:decay' },
    { key: 'predelay', label: 'Pre-delay', bind: 'fx:param:predelay' },
    { key: 'size', label: 'Size', bind: 'fx:param:size' },
    { key: 'hidamp', label: 'Hi Damp', bind: 'fx:param:hidamp' },
    { key: 'mix', label: 'Mix', bind: 'fx:param:mix' },
  ],
  Room: [
    { key: 'decay', label: 'Decay', bind: 'fx:param:decay' },
    { key: 'predelay', label: 'Pre-delay', bind: 'fx:param:predelay' },
    { key: 'size', label: 'Size', bind: 'fx:param:size' },
    { key: 'hidamp', label: 'Hi Damp', bind: 'fx:param:hidamp' },
    { key: 'mix', label: 'Mix', bind: 'fx:param:mix' },
  ],
  Delay: [
    { key: 'time', label: 'Time', bind: 'fx:param:time' },
    { key: 'feedback', label: 'Feedback', bind: 'fx:param:feedback' },
    { key: 'hicut', label: 'Hi Cut', bind: 'fx:param:hicut' },
    { key: 'spread', label: 'Spread', bind: 'fx:param:spread' },
    { key: 'mix', label: 'Mix', bind: 'fx:param:mix' },
  ],
  Chorus: [
    { key: 'rate', label: 'Rate', bind: 'fx:param:rate' },
    { key: 'depth', label: 'Depth', bind: 'fx:param:depth' },
    { key: 'spread', label: 'Spread', bind: 'fx:param:spread' },
    { key: 'mix', label: 'Mix', bind: 'fx:param:mix' },
  ],
  Flanger: [
    { key: 'rate', label: 'Rate', bind: 'fx:param:rate' },
    { key: 'depth', label: 'Depth', bind: 'fx:param:depth' },
    { key: 'feedback', label: 'Feedback', bind: 'fx:param:feedback' },
    { key: 'mix', label: 'Mix', bind: 'fx:param:mix' },
  ],
};

/**
 * Knob descriptors for a type, filtered by desk caps (honest UI: knobs
 * whose param cap is absent are removed). caps = bind ids the desk
 * supports, or null for the full fixture catalogue.
 */
export function fxKnobList(fxType: string, caps: string[] | null): FxKnobParamDef[] {
  const defs = FX_PARAM_DEFS[fxType];
  if (!defs) throw new Error('unknown fx type: ' + fxType);
  const labels = fxKnobDefs(fxType).map(d => d.label);
  assertLabelsMatch(fxType, defs.map(d => d.label), labels);
  if (caps == null) return defs.slice();
  return defs.filter(d => caps.includes(d.bind));
}

function assertLabelsMatch(fxType: string, ours: string[], spec: string[]): void {
  if (ours.length !== spec.length || ours.some((l, i) => l !== spec[i])) {
    throw new Error(`fx knob labels for ${fxType} drifted from fxKnobDefs: ${ours.join(',')}`);
  }
}

export interface FxSlotVm {
  index: number;
  name: string;
  /** Display name (full effect name online; design label offline). */
  type: string;
  /** Desk short code (`caps.fx` id, e.g. 'DLY'); '' offline/design. */
  code: string;
  /** Family visual for the module tile. */
  art: FxArt;
  state: string;
  stateColor: string;
  pressed: boolean;
  border: string;
  bg: string;
  ret: string;
  /** Meter fill width in % (0..100, one decimal), as text for the style. */
  meterPct: string;
}

function meterPctText(on: boolean, levelDb: number | null): string {
  if (!on) return '0';
  return (dbToPos(levelDb ?? -90) * 100).toFixed(1);
}

export function buildFxSlotVm(
  slot: FxSlot,
  index: number,
  selected: boolean,
  levelDb: number | null,
): FxSlotVm {
  return {
    index,
    name: 'FX ' + (index + 1),
    type: slot.type,
    code: '',
    art: fxArtFor(slot.type),
    state: fxStateLabel(slot),
    stateColor: slot.on ? '#7be27b' : '#f0b44a',
    pressed: selected,
    border: selected ? '#b28cff' : 'var(--u4-line-faint)',
    bg: selected ? '#1b2330' : 'var(--u4-panel-bg)',
    ret: fxRetLabel(index),
    meterPct: meterPctText(slot.on, levelDb),
  };
}

/**
 * Slot cards for the row. Slots beyond the desk's count are removed
 * (honest UI): slotCount = desk's FX rack size, or null for the full 8.
 */
export function buildFxSlots(
  fx: FxSlot[],
  fxSel: number,
  levelOf: ((id: string) => number | null) | null,
  slotCount: number | null = null,
): FxSlotVm[] {
  const n = slotCount ?? fx.length;
  const out: FxSlotVm[] = [];
  for (let k = 0; k < Math.min(n, fx.length); k++) {
    out.push(buildFxSlotVm(fx[k], k, k === fxSel, levelOf ? levelOf('fx' + (2 * k + 1)) : null));
  }
  return out;
}

export function fxTitle(fxSel: number, slot: FxSlot): string {
  return fxTitleText(fxSel, slot);
}

export interface FxToggleVm {
  label: string;
  pressed: boolean;
  bg: string;
  fg: string;
  border: string;
}

export function fxToggleVm(slot: FxSlot): FxToggleVm {
  return {
    label: slot.on ? 'On' : 'Bypassed',
    pressed: slot.on,
    bg: slot.on ? '#5cdb5c' : '#2a1f0a',
    fg: slot.on ? '#10240f' : '#f6c76a',
    border: slot.on ? '#5cdb5c' : '#f0b44a',
  };
}

export interface FxTypeVm {
  /** Button text (full effect name online; design label offline). */
  label: string;
  /** Wire key sent on pick (`caps.fx` id; design label offline). */
  value: string;
  /** Desk short code caption (`caps.fx` id); '' offline/design. */
  code: string;
  /** Family visual for the button dot. */
  art: FxArt;
  pressed: boolean;
  bg: string;
  fg: string;
}

export function fxTypeButtons(curType: string): FxTypeVm[] {
  return FX_TYPES.map(t => ({
    label: t,
    value: t,
    code: '',
    art: fxArtFor(t),
    pressed: t === curType,
    bg: t === curType ? '#b28cff' : 'var(--u4-btn-bg)',
    fg: t === curType ? '#1a0f2e' : 'var(--u4-text-4)',
  }));
}

export function fxIsDelay(slot: FxSlot): boolean {
  return slot.type === 'Delay';
}

export function fxTapLabel(slot: FxSlot): string {
  return slot.bpm ? 'Tap · ' + slot.bpm + ' BPM' : 'Tap tempo';
}

/** Knob value text for param pi of a slot (design fxDefs formatters). */
export function fxKnobValueText(slot: FxSlot, pi: number): string {
  const defs = fxKnobDefs(slot.type);
  if (pi < 0 || pi >= defs.length) throw new Error(`fx param ${pi} out of range for ${slot.type}`);
  return defs[pi].fmt(slot.p[pi]);
}

/** Knob aria label, e.g. `Decay 4.3 s` (design knobItem aria). */
export function fxKnobAria(slot: FxSlot, pi: number): string {
  return fxKnobDefs(slot.type)[pi].label + ' ' + fxKnobValueText(slot, pi);
}

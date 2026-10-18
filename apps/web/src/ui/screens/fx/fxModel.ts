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
  type: string;
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
  label: string;
  pressed: boolean;
  bg: string;
  fg: string;
}

export function fxTypeButtons(curType: string): FxTypeVm[] {
  return FX_TYPES.map(t => ({
    label: t,
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

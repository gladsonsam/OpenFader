// C3 Gate+Compressor package: pure transfer-graph geometry + dyn view data.
//
// Exact port of dynVals in docs/design/src/design-template.html (lines
// 1149-1179) over the gateOut/compOut math in fixtures/designData.ts (itself
// a port of design-logic.js.txt). Everything here is rune-free and DOM-free
// so it is node-testable; TransferGraph.svelte paints these values.
//
// Design facts encoded:
//   S = 200 (viewBox 0 0 200 200), map(v) = (v - lo) / -lo * S,
//   lo = -80 (gate) | -60 (comp); 81 curve samples; grid every 10 dB;
//   gate axes [-80 -60 -40 -20 0], comp axes [-60 -45 -30 -15 0];
//   GR scale 80 dB (gate) | 24 dB (comp); knob colour #3ed0e0 | #ff9f43;
//   curve stroke #f0d36a, fill colour + '1c'.
// Drag note: the design implements NO graph drag (threshold tag, dot and
// curve are plain spans/paths; only the knobs drag, kind 'g', 180 px
// full-scale). Threshold "interaction" is the Threshold knob.

import {
  clamp,
  compOut,
  compSummary,
  fromN,
  gateOut,
  gateSummary,
  spec,
  toN,
  type Comp,
  type Gate,
} from '../../../fixtures/designData.ts';

export type DynKind = 'gate' | 'comp';

export const DYN_SIZE = 200;
export const DYN_LO: Record<DynKind, number> = { gate: -80, comp: -60 };
export const DYN_COLOR: Record<DynKind, string> = { gate: '#3ed0e0', comp: '#ff9f43' };
export const DYN_CURVE_STROKE = '#f0d36a';

export type DynParams = Gate | Comp;

export function dynMap(lo: number, v: number): number {
  return ((v - lo) / -lo) * DYN_SIZE;
}

function outOf(kind: DynKind, p: DynParams, x: number): number {
  return kind === 'gate' ? gateOut(p as Gate, x) : compOut(p as Comp, x);
}

/** 81-sample transfer curve path (design pts loop, i = 0..80). */
export function dynCurvePath(kind: DynKind, p: DynParams): string {
  const lo = DYN_LO[kind];
  const pts: string[] = [];
  for (let i = 0; i <= 80; i++) {
    const x = lo + ((-lo) * i) / 80;
    pts.push(
      dynMap(lo, x).toFixed(1) + ' ' + (DYN_SIZE - clamp(dynMap(lo, outOf(kind, p, x)), -4, DYN_SIZE + 4)).toFixed(1),
    );
  }
  return 'M' + pts.join(' L');
}

/** Curve path closed to the plot bottom (design dvFill). */
export function dynFillPath(kind: DynKind, p: DynParams): string {
  return dynCurvePath(kind, p) + ' L200 200 L0 200 Z';
}

export function dynFillColor(kind: DynKind): string {
  return DYN_COLOR[kind] + '1c';
}

export interface DynGridLine {
  p: string;
  q: string;
}

/** Grid lines every 10 dB from lo+10 to -10 (design grid loop). */
export function dynGrid(lo: number): DynGridLine[] {
  const grid: DynGridLine[] = [];
  for (let v = lo + 10; v < 0; v += 10) grid.push({ p: dynMap(lo, v).toFixed(1), q: (DYN_SIZE - dynMap(lo, v)).toFixed(1) });
  return grid;
}

export interface DynAxisTick {
  label: string;
  left: string;
  top: string;
  shift: string;
}

export function dynAxis(kind: DynKind): DynAxisTick[] {
  const lo = DYN_LO[kind];
  const vals = kind === 'gate' ? [-80, -60, -40, -20, 0] : [-60, -45, -30, -15, 0];
  return vals.map((v, i, arr) => ({
    label: String(v),
    left: (dynMap(lo, v) / 2).toFixed(1),
    top: (100 - dynMap(lo, v) / 2).toFixed(1),
    shift: i === 0 ? '0' : i === arr.length - 1 ? '-100%' : '-50%',
  }));
}

export interface DynThreshold {
  /** Threshold line x in viewBox units. */
  x: string;
  /** Tag left in % of plot width (clamped 8..92). */
  leftPct: string;
  text: string;
}

export function dynThreshold(p: { thr: number }, lo: number): DynThreshold {
  return {
    x: dynMap(lo, p.thr).toFixed(1),
    leftPct: clamp(dynMap(lo, p.thr) / 2, 8, 92).toFixed(1),
    text: p.thr.toFixed(1) + ' dB',
  };
}

/** Draggable graph handles (reference-hardware circles): T rides the knee
 * for both kinds; comp also gets R (ratio) at the top-right curve end and
 * G (makeup gain) at the bottom-left curve start. Positions ride the live
 * curve exactly (makeup shifts the output) — centres are NOT clamped, so
 * the circles always sit on the line; the plot allows overflow so edge
 * handles stay fully visible and grabbable. */
export interface DynHandle {
  /** Param edited by this handle (event name + dyn prop). */
  id: 'thr' | 'ratio' | 'makeup';
  /** Letter painted in the circle (G / T / R). */
  label: string;
  /** Centre in % of plot width/height. */
  left: number;
  top: number;
  /** Current raw value (spec units: dB, or ratio). */
  value: number;
  /** Accessible stem, e.g. 'Compressor threshold handle'. */
  aria: string;
}

export function dynHandles(kind: DynKind, p: DynParams): DynHandle[] {
  const lo = DYN_LO[kind];
  const at = (x: number): { left: number; top: number } => ({
    left: clamp(dynMap(lo, x) / 2, 0, 100),
    top: clamp(100 - clamp(dynMap(lo, outOf(kind, p, x)), 0, DYN_SIZE) / 2, 0, 100),
  });
  const name = kind === 'gate' ? 'Gate' : 'Compressor';
  if (kind === 'gate') {
    const g = p as Gate;
    const t = at(g.thr);
    return [{ id: 'thr', label: 'T', ...t, value: g.thr, aria: name + ' threshold handle' }];
  }
  const c = p as Comp;
  const t = at(c.thr);
  const r = at(0);
  const g = at(lo);
  return [
    { id: 'makeup', label: 'G', ...g, value: c.makeup, aria: name + ' makeup handle' },
    { id: 'thr', label: 'T', ...t, value: c.thr, aria: name + ' threshold handle' },
    { id: 'ratio', label: 'R', ...r, value: c.ratio, aria: name + ' ratio handle' },
  ];
}

/** Raw threshold dB from a horizontal plot fraction (handle or plot drag). */
export function dynThrRaw(kind: DynKind, fracX: number): number {
  const f = clamp(fracX, 0, 1);
  return Math.round((DYN_LO[kind] + f * -DYN_LO[kind]) * 2) / 2;
}

/** Raw ratio from a vertical plot fraction (0 = top): follow-the-line —
 * the curve end tracks the finger, like G (makeup) and T (threshold)
 * already do. Solves r from the end-point equation
 * out(0) = thr + (0-thr)/r + makeup for the finger's output level and
 * quantizes through the ratio spec. Below knee height there is nothing
 * further to follow, so it pins at 20:1. (Absolute mapping — up = more
 * ratio — would drive the line end opposite the finger.) */
export function dynRatioRaw(fracY: number, thr: number, makeup: number): number {
  const yOut = DYN_LO.comp + (1 - clamp(fracY, 0, 1)) * -DYN_LO.comp;
  const denom = yOut - makeup - thr;
  const r = denom <= 0 ? 20 : (0 - thr) / denom;
  const s = spec('ratio');
  return fromN(s, toN(s, clamp(r, s.min, s.max)));
}

/** Raw makeup dB from a vertical plot fraction (0 = top). */
export function dynMakeupRaw(fracY: number): number {
  return fromN(spec('makeup'), clamp(1 - fracY, 0, 1));
}

export interface DynDot {
  left: number;
  top: number;
}

/** Operating-point dot in % of plot size (numeric; format with toFixed(1)). */
export function dynDotNums(kind: DynKind, p: DynParams, levelDb: number): DynDot {
  const lo = DYN_LO[kind];
  const lvl = clamp(levelDb, lo, 0);
  const outL = outOf(kind, p, lvl);
  return {
    left: dynMap(lo, lvl) / 2,
    top: 100 - clamp(dynMap(lo, outL), 0, DYN_SIZE) / 2,
  };
}

export function dynDot(
  kind: DynKind,
  p: DynParams,
  levelDb: number,
): { leftPct: string; topPct: string } {
  const d = dynDotNums(kind, p, levelDb);
  return { leftPct: d.left.toFixed(1), topPct: d.top.toFixed(1) };
}

export interface DynGr {
  db: number;
  pct: number;
  text: string;
}

/** Gain-reduction readout (design grDb / dvGrPct / dvGrText). */
export function dynGrNums(kind: DynKind, p: DynParams, levelDb: number): DynGr {
  const lo = DYN_LO[kind];
  const lvl = clamp(levelDb, lo, 0);
  let grDb: number;
  if (kind === 'gate') {
    const g = p as Gate;
    grDb = g.on && lvl < g.thr ? g.range : 0;
  } else {
    const c = p as Comp;
    grDb = Math.max(0, lvl - (outOf(kind, p, lvl) - c.makeup));
  }
  return {
    db: grDb,
    pct: clamp((grDb / (kind === 'gate' ? 80 : 24)) * 100, 0, 100),
    text: grDb > 0.05 ? '-' + grDb.toFixed(1) : '0.0',
  };
}

export function dynGr(
  kind: DynKind,
  p: DynParams,
  levelDb: number,
): { pct: string; text: string } {
  const g = dynGrNums(kind, p, levelDb);
  return { pct: g.pct.toFixed(1), text: g.text };
}

export interface DynToggle {
  bg: string;
  fg: string;
  label: string;
  pressed: string;
}

export function dynToggle(kind: DynKind, on: boolean): DynToggle {
  return {
    bg: on ? '#5cdb5c' : 'var(--u4-btn-bg)',
    fg: on ? '#10240f' : 'var(--u4-text-4)',
    label: (kind === 'gate' ? 'Gate ' : 'Compressor ') + (on ? 'In' : 'Bypassed'),
    pressed: on ? 'true' : 'false',
  };
}

export function dynSummary(kind: DynKind, g: Gate, c: Comp): string {
  return kind === 'gate' ? gateSummary(g) : compSummary(c);
}

export function dynAria(kind: DynKind): string {
  return (kind === 'gate' ? 'Gate' : 'Compressor') + ' transfer curve';
}

export interface DynKnobDef {
  /** Channel dyn prop edited (thr | ratio | knee | att | rel | makeup | range | hold). */
  prop: string;
  label: string;
  /** fixtures/designData spec key (drag math + formatting). */
  specKey: string;
  /** designData catalogue bind suffix, e.g. 'param:comp_thr'. */
  bind: string;
  /** Double-tap reset target (design K() defaults). */
  defaultValue: number;
}

/** Knob grids as data (design K() tables); users add/remove by editing the list. */
export const DYN_KNOBS: Record<DynKind, DynKnobDef[]> = {
  gate: [
    { prop: 'thr', label: 'Threshold', specKey: 'gthr', bind: 'param:gate_thr', defaultValue: -50 },
    { prop: 'range', label: 'Range', specKey: 'range', bind: 'param:gate_range', defaultValue: 40 },
    { prop: 'att', label: 'Attack', specKey: 'gatt', bind: 'param:gate_att', defaultValue: 0.5 },
    { prop: 'hold', label: 'Hold', specKey: 'hold', bind: 'param:gate_hold', defaultValue: 50 },
    { prop: 'rel', label: 'Release', specKey: 'grel', bind: 'param:gate_rel', defaultValue: 150 },
  ],
  comp: [
    { prop: 'thr', label: 'Threshold', specKey: 'cthr', bind: 'param:comp_thr', defaultValue: -20 },
    { prop: 'ratio', label: 'Ratio', specKey: 'ratio', bind: 'param:comp_ratio', defaultValue: 3 },
    { prop: 'knee', label: 'Knee', specKey: 'knee', bind: 'param:comp_knee', defaultValue: 3 },
    { prop: 'att', label: 'Attack', specKey: 'catt', bind: 'param:comp_att', defaultValue: 10 },
    { prop: 'rel', label: 'Release', specKey: 'crel', bind: 'param:comp_rel', defaultValue: 120 },
    { prop: 'makeup', label: 'Makeup', specKey: 'makeup', bind: 'param:comp_makeup', defaultValue: 0 },
  ],
};

/** Raw knob value for a prop from the live gate/comp params. */
export function dynKnobValue(kind: DynKind, g: Gate, c: Comp, prop: string): number {
  const src = (kind === 'gate' ? g : c) as unknown as Record<string, number>;
  return src[prop];
}

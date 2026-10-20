// C2 EQ package: pure view-models for the HPF + EQ tab.
//
// Exact port of eqVals() in docs/design/src/design-logic.js.txt plus the
// inline styles in docs/design/src/design-template.html (isEqTab section).
// Everything here is rune-free and DOM-free so it is node-testable; the
// Svelte components under this folder only render these VMs and forward
// pointer/keyboard gestures to the store (ui.do.* in EqTab.svelte).
//
// Curve math (RBJ biquads @ 48 kHz) is reused from fixtures/designData.ts
// (filters/resp), never re-derived.

import {
  clamp,
  eqXOf,
  filters,
  fmtF,
  fmtG,
  resp,
  type Eq,
  type EqBand,
  type EqBandType,
  type Hpf,
} from '../../../fixtures/designData.ts';

/** Node colours, bands 0..4 (HPF, Low, Lo-Mid, Hi-Mid, High). */
export const EQ_BAND_COLORS = ['#d7dde8', '#ff9f43', '#f6d743', '#3ed0e0', '#b28cff'];

export const EQ_BAND_NAMES = ['HPF', 'Low', 'Lo-Mid', 'Hi-Mid', 'High'];

/** SVG graph geometry (design viewBox 0 0 1000 300). */
export const EQ_GRAPH_W = 1000;
export const EQ_GRAPH_H = 300;
/** Design path resolution (N = 180 segments). */
export const EQ_GRAPH_N = 180;

export interface EqGraphOpts {
  /** Log-frequency axis limits (design: 20..20000). */
  freqMin?: number;
  freqMax?: number;
  /** dB axis limits, top..bottom (design: +15..-15; labels show +-12). */
  dbTop?: number;
  dbBottom?: number;
  /** Path segments (design: 180). Lower for custom thumbnails. */
  segments?: number;
  /** Node/accent colours per band, index 0 = HPF. */
  colors?: string[];
  /** Bands rendered (design: all of eq.bands; slice for custom graphs). */
  bandCount?: number;
}

export interface EqGridLine {
  x: string;
  stroke: string;
}

export interface EqGridLineH {
  y: string;
  stroke: string;
}

export interface EqAxisLabel {
  x: string;
  label: string;
}

export interface EqGainLabel {
  y: string;
  label: string;
}

export interface EqNodeVm {
  band: number;
  label: string;
  aria: string;
  valueText: string;
  freq: number;
  gain: number;
  x: string;
  y: string;
  size: number;
  ring: string;
  color: string;
  op: number;
}

export interface EqBandGhost {
  band: number;
  name: string;
  d: string;
  fill: string;
  stroke: string;
  on: boolean;
  color: string;
}

export interface EqGraphVm {
  curvePath: string;
  fillPath: string;
  curveStroke: string;
  curveFill: string;
  bandFill: string;
  bandFillColor: string;
  bandStroke: string;
  bandGhosts: EqBandGhost[];
  gridV: EqGridLine[];
  gridH: EqGridLineH[];
  freqLabels: EqAxisLabel[];
  gainLabels: EqGainLabel[];
  nodes: EqNodeVm[];
}

export interface EqTypeOption {
  value: EqBandType;
  label: string;
  active: boolean;
}

export interface EqChip {
  label: string;
  pressed: 'true' | 'false';
  bg: string;
  fg: string;
}

export function eqChip(label: string, active: boolean): EqChip {
  return {
    label,
    pressed: active ? 'true' : 'false',
    bg: active ? 'var(--u4-choice-on-bg, var(--u4-text-1))' : 'var(--u4-btn-bg)',
    fg: active ? 'var(--u4-seg-on-fg)' : 'var(--u4-text-4)',
  };
}

export interface EqOnOff {
  onStr: 'true' | 'false';
  onBg: string;
  onText: string;
  onLabel: string;
  toggleAria: string;
}

export function eqOnOff(on: boolean, name: string): EqOnOff {
  return {
    onStr: on ? 'true' : 'false',
    onBg: on ? '#5cdb5c' : 'var(--u4-btn-bg)',
    onText: on ? '#10240f' : 'var(--u4-text-5)',
    onLabel: on ? 'On' : 'Off',
    toggleAria: name + ' on/off',
  };
}

export interface EqBandCardVm extends EqOnOff {
  kind: 'band';
  band: number;
  name: string;
  color: string;
  selected: boolean;
  border: string;
  types: EqChip[];
  typeValues: EqBandType[];
  freq: number;
  gain: number;
  q: number;
}

export interface EqHpfCardVm extends EqOnOff {
  kind: 'hpf';
  band: 0;
  name: 'HPF';
  color: string;
  selected: boolean;
  border: string;
  slope: 12 | 24;
  freq: number;
}

/** Type segmented options per band index (design typeSets table). */
export function eqTypeOptions(bandIndex: number, type: EqBandType): { options: EqChip[]; values: EqBandType[] } {
  let defs: Array<[EqBandType, string]>;
  if (bandIndex === 0) defs = [['LS', 'Shelf'], ['PEQ', 'Bell']];
  else if (bandIndex === 3) defs = [['PEQ', 'Bell'], ['HS', 'Shelf']];
  else defs = [['PEQ', 'Bell']];
  return {
    options: defs.map(([v, label]) => eqChip(label, type === v)),
    values: defs.map(([v]) => v),
  };
}

function bandName(band: number): string {
  return EQ_BAND_NAMES[band] ?? 'Band ' + band;
}

function bandColor(band: number, colors: string[]): string {
  return colors[band] ?? 'var(--u4-text-1)';
}

function visibleBands(eq: Eq, bandCount: number): EqBand[] {
  return eq.bands.slice(0, Math.max(0, bandCount));
}

/** Summed magnitude response of the whole EQ at f (dB), for tests/overlays. */
export function eqResponseAt(eq: Eq | null, f: number): number {
  const fl = filters(eq);
  return fl.length ? resp(fl, f) : 0;
}

/**
 * Port of the node branch of move(): normalised graph position -> freq/gain.
 * HPF clamping (min 1000 Hz) happens in eqNodeApply, mirroring the design's
 * `Math.min(1000, f)` inside updCh.
 */
export function eqPointFromPos(x: number, y: number): { f: number; g: number } {
  const cx = clamp(x, 0, 1), cy = clamp(y, 0, 1);
  return {
    f: 20 * Math.pow(1000, cx),
    g: Math.round((15 - 30 * cy) * 10) / 10,
  };
}

/** Apply a graph-drag/arrow point to a band (HPF: freq only, clamped to 1000 Hz). */
export function eqNodeApply(band: number, f: number, g: number): { f: number; g: number } {
  if (band === 0) return { f: clamp(Math.min(1000, f), 20, 1000), g: -3 };
  return { f: clamp(f, 20, 20000), g: Math.round(clamp(g, -15, 15) * 10) / 10 };
}

/** Port of the selInfo block in eqVals. */
export function eqSelInfoText(eq: Eq, selBand: number): string {
  if (selBand === 0) {
    const h = eq.hpf;
    return 'HPF · ' + fmtF(h.f) + ' · ' + h.slope + ' dB/oct' + (h.on ? '' : ' · off');
  }
  const b = eq.bands[selBand - 1];
  if (!b) return '';
  return (
    bandName(selBand) + ' · ' + fmtF(b.f) + ' · ' + fmtG(b.g) + ' · Q ' + b.q.toFixed(2) + (b.on ? '' : ' · off')
  );
}

export interface EqMasterToggle {
  str: 'true' | 'false';
  bg: string;
  text: string;
  label: string;
}

/** Port of the EQ In / EQ Bypassed master toggle visuals in eqVals. */
export function eqMasterToggle(eq: Eq): EqMasterToggle {
  return eq.on
    ? { str: 'true', bg: '#5cdb5c', text: '#10240f', label: 'EQ In' }
    : { str: 'false', bg: 'var(--u4-btn-bg)', text: 'var(--u4-text-4)', label: 'EQ Bypassed' };
}

/** Port of eqVals(): every path, grid line, label, node and card VM. */
export function eqGraphVm(eq: Eq, selBand: number, opts: EqGraphOpts = {}): EqGraphVm {
  const W = EQ_GRAPH_W, H = EQ_GRAPH_H;
  const N = opts.segments ?? EQ_GRAPH_N;
  const fMin = opts.freqMin ?? 20, fMax = opts.freqMax ?? 20000;
  const dbTop = opts.dbTop ?? 15, dbBottom = opts.dbBottom ?? -15;
  const colors = opts.colors ?? EQ_BAND_COLORS;
  const bands = visibleBands(eq, opts.bandCount ?? eq.bands.length);
  const sb = clamp(Math.round(selBand), 0, bands.length);

  const xOf = (f: number): number => Math.log(f / fMin) / Math.log(fMax / fMin);
  const toY = (db: number): number => clamp(((dbTop - db) / (dbTop - dbBottom)) * H, -20, H + 20);
  const scoped: Eq = { on: eq.on, hpf: eq.hpf, bands };
  const pathFor = (fl: ReturnType<typeof filters>): string => {
    const pts: string[] = [];
    for (let i = 0; i <= N; i++) {
      const f = fMin * Math.pow(fMax / fMin, i / N);
      pts.push(((i / N) * W).toFixed(1) + ' ' + toY(fl.length ? resp(fl, f) : 0).toFixed(1));
    }
    return 'M' + pts.join(' L');
  };

  const all = filters(scoped);
  const curvePath = pathFor(all);
  const zeroY = H / 2;
  const fillPath = curvePath + ' L' + W + ' ' + zeroY + ' L0 ' + zeroY + ' Z';

  let own: ReturnType<typeof filters> = [];
  if (eq.on) {
    if (sb === 0 && eq.hpf.on) own = filters({ on: true, hpf: eq.hpf, bands: [] });
    else if (sb > 0 && bands[sb - 1]?.on) {
      own = filters({ on: true, hpf: { on: false, f: 80, slope: 12 } as Hpf, bands: [bands[sb - 1]] });
    }
  }
  const bandFill = own.length ? pathFor(own) + ' L' + W + ' ' + zeroY + ' L0 ' + zeroY + ' Z' : '';

  // Per-band ghosts: forced-on single-band shape so the curve exists even
  // when the band (or the whole EQ) is off/bypassed. Audible ghosts reuse
  // the band colour; off/bypassed ghosts fall back to dim grey.
  const OFF_GHOST_FILL = 'rgba(90,98,114,0.14)';
  const OFF_GHOST_STROKE = '#5a6272';
  const bandGhosts: EqBandGhost[] = [];
  const ghostCount = bands.length + 1;
  for (let bIdx = 0; bIdx < ghostCount; bIdx++) {
    const bandOn = bIdx === 0 ? eq.hpf.on : (bands[bIdx - 1]?.on ?? false);
    const audible = eq.on && bandOn;
    let fl: ReturnType<typeof filters> = [];
    if (bIdx === 0) {
      fl = filters({ on: true, hpf: { ...eq.hpf, on: true }, bands: [] });
    } else {
      const b = bands[bIdx - 1];
      if (b) {
        fl = filters({
          on: true,
          hpf: { on: false, f: 80, slope: 12 } as Hpf,
          bands: [{ ...b, on: true }],
        });
      }
    }
    const color = bandColor(bIdx, colors);
    bandGhosts.push({
      band: bIdx,
      name: bandName(bIdx),
      d: fl.length ? pathFor(fl) + ' L' + W + ' ' + zeroY + ' L0 ' + zeroY + ' Z' : '',
      fill: audible ? color + '2e' : OFF_GHOST_FILL,
      stroke: audible ? color : OFF_GHOST_STROKE,
      on: audible,
      color,
    });
  }

  const gridV: EqGridLine[] = [];
  [10, 100, 1000, 10000].forEach(dd => {
    for (let m = 1; m <= 9; m++) {
      const f = dd * m;
      if (f < fMin || f > fMax) continue;
      gridV.push({
        x: (xOf(f) * W).toFixed(1),
        stroke: m === 1 || m === 2 || m === 5 ? 'var(--u4-graph-major, #223047)' : 'var(--u4-graph-minor, #141c2a)',
      });
    }
  });
  const gridH: EqGridLineH[] = [12, 6, 0, -6, -12].map(g => ({
    y: toY(g).toFixed(1),
    stroke: g === 0 ? 'var(--u4-graph-unity)' : 'var(--u4-graph-major, #1b2536)',
  }));
  const freqLabels: EqAxisLabel[] = [
    [50, '50'], [100, '100'], [200, '200'], [500, '500'],
    [1000, '1k'], [2000, '2k'], [5000, '5k'], [10000, '10k'],
  ].map(([f, label]) => ({ x: (xOf(f as number) * 100).toFixed(2), label: label as string }));
  const gainLabels: EqGainLabel[] = [12, 6, 0, -6, -12].map(g => ({
    y: (((dbTop - g) / (dbTop - dbBottom)) * 100).toFixed(2),
    label: (g > 0 ? '+' : '') + g,
  }));

  const nodes: EqNodeVm[] = [];
  const mkNode = (band: number, f: number, g: number, on: boolean): void => {
    const active = sb === band;
    const yN = clamp((dbTop - g) / (dbTop - dbBottom), 0.03, 0.97);
    const audible = on && eq.on;
    nodes.push({
      band,
      label: band === 0 ? 'H' : String(band),
      aria:
        bandName(band) +
        ' node, ' +
        fmtF(f) +
        (band ? ', ' + fmtG(g) : '') +
        (audible ? '' : ' · off'),
      valueText: band === 0 ? fmtF(f) : fmtF(f) + ', ' + fmtG(g),
      freq: f,
      gain: g,
      x: (clamp(xOf(f), 0, 1) * 100).toFixed(2),
      y: (yN * 100).toFixed(2),
      size: active ? 36 : 30,
      ring: active ? '#ffffff' : 'rgba(0,0,0,.55)',
      color: bandColor(band, colors),
      op: on && eq.on ? 1 : 0.4,
    });
  };
  mkNode(0, eq.hpf.f, -3, eq.hpf.on);
  bands.forEach((b, i) => mkNode(i + 1, b.f, b.g, b.on));

  return {
    curvePath,
    fillPath,
    curveStroke: eq.on ? 'var(--u4-detail-curve, #f0d36a)' : '#5a6272',
    curveFill: eq.on ? 'rgba(240,211,106,0.10)' : 'none',
    bandFill,
    bandFillColor: own.length ? bandColor(sb, colors) + '2e' : 'none',
    bandStroke: own.length ? bandColor(sb, colors) : 'none',
    bandGhosts,
    gridV,
    gridH,
    freqLabels,
    gainLabels,
    nodes,
  };
}

/** Port of the cards block in eqVals(). */
export function eqHpfCardVm(eq: Eq, selBand: number, colors: string[] = EQ_BAND_COLORS): EqHpfCardVm {
  return {
    kind: 'hpf',
    band: 0,
    name: 'HPF',
    color: colors[0] ?? '#d7dde8',
    selected: selBand === 0,
    border: selBand === 0 ? (colors[0] ?? '#d7dde8') : 'var(--u4-line-panel)',
    slope: eq.hpf.slope,
    freq: eq.hpf.f,
    ...eqOnOff(eq.hpf.on, 'HPF'),
  };
}

export function eqBandCardsVm(
  eq: Eq,
  selBand: number,
  bandCount?: number,
  colors: string[] = EQ_BAND_COLORS,
): EqBandCardVm[] {
  const bands = visibleBands(eq, bandCount ?? eq.bands.length);
  return bands.map((b, i) => {
    const band = i + 1;
    const { options, values } = eqTypeOptions(i, b.type);
    return {
      kind: 'band',
      band,
      name: bandName(band),
      color: colors[band] ?? 'var(--u4-text-1)',
      selected: selBand === band,
      border: selBand === band ? (colors[band] ?? 'var(--u4-text-1)') : 'var(--u4-line-panel)',
      types: options,
      typeValues: values,
      freq: b.f,
      gain: b.g,
      q: b.q,
      ...eqOnOff(b.on, bandName(band)),
    };
  });
}

/** Knob normalisations, mirroring Knob.svelte eq mode (kept here for tests). */
export function eqFreqNorm(f: number, hpf: boolean): number {
  return hpf ? Math.log(f / 20) / Math.log(50) : eqXOf(f);
}

export { clamp as eqClamp };

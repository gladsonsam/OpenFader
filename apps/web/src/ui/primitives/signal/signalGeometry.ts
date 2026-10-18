// P1 Signal primitives: pure geometry tables, pixel-copied from
// docs/design/src/design-template.html (renderVals marks, stripVals,
// faderDown, knobItem). Components import these; node tests assert them.
//
// Run: node --experimental-strip-types apps/web/src/ui/primitives/signal/signal.test.ts

/** Fader scale marks: pct = pos * 100 (renderVals `marks`). */
export interface ScaleMark {
  pct: number;
  label: string;
}

export const SCALE_MARKS: ScaleMark[] = [
  { pct: 100, label: '10' },
  { pct: 87.5, label: '5' },
  { pct: 75, label: '0' },
  { pct: 62.5, label: '-5' },
  { pct: 50, label: '-10' },
  { pct: 37.5, label: '-20' },
  { pct: 25, label: '-30' },
  { pct: 12.5, label: '-50' },
];

/** Tick colours: the '0' tick uses --u4-text-1, the rest #566175. */
export function scaleTickColor(label: string): string {
  return label === '0' ? 'var(--u4-text-1)' : '#566175';
}

export type FaderVariant = 'strip' | 'aside' | 'master';

export interface FaderGeometry {
  insets: { l: number; r: number; t: number; b: number };
  railLeft: string;
  railWidth: number;
  cap: { w: number; h: number; gripLR: number; gripTB: number; gripH: number; left: string };
  readH: number;
  readFont: number;
  scale: 'strip' | 'aside';
  meter: 'strip' | 'aside' | 'master';
}

/** Per-variant fader anatomy (mixer strip / channel strip / master). */
export function faderGeometry(variant: FaderVariant): FaderGeometry {
  if (variant === 'aside') {
    return {
      insets: { l: 6, r: 6, t: 32, b: 34 },
      railLeft: 'calc(50% - 2px)',
      railWidth: 4,
      cap: { w: 46, h: 60, gripLR: 5, gripTB: 6, gripH: 17, left: '50%' },
      readH: 18,
      readFont: 13,
      scale: 'aside',
      meter: 'aside',
    };
  }
  if (variant === 'master') {
    return {
      insets: { l: 2, r: 2, t: 26, b: 28 },
      railLeft: 'calc(50% - 6px)',
      railWidth: 4,
      cap: { w: 40, h: 52, gripLR: 4, gripTB: 5, gripH: 15, left: 'calc(50% - 4px)' },
      readH: 15,
      readFont: 12,
      scale: 'strip',
      meter: 'master',
    };
  }
  return {
    insets: { l: 0, r: 0, t: 26, b: 28 },
    railLeft: 'calc(50% - 2px)',
    railWidth: 4,
    cap: { w: 36, h: 50, gripLR: 4, gripTB: 5, gripH: 14, left: '50%' },
    readH: 15,
    readFont: 12,
    scale: 'strip',
    meter: 'strip',
  };
}

/** EQ knob reset defaults, mirroring uiLogic.resetEqParam (F/Q tables). */
export const EQ_RESET_F = [80, 100, 400, 2500, 8000];
export const EQ_RESET_Q = [0.7, 0.7, 1.4, 1.4, 0.7];

/** Pan marker geometry: left = 50 + pan * 44 (stripVals panLeft). */
export function panMarkerLeft(pan: number): number {
  return 50 + pan * 44;
}

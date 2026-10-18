// P1 Signal primitives: node tests for the pure geometry/drag tables.
// Run: node --experimental-strip-types apps/web/src/ui/primitives/signal/signal.test.ts

import { strict as assert } from 'node:assert';
import {
  SCALE_MARKS,
  scaleTickColor,
  faderGeometry,
  EQ_RESET_F,
  EQ_RESET_Q,
  panMarkerLeft,
} from './signalGeometry.ts';
import {
  dbToPos,
  fmtDb,
  knobDash,
  knobRot,
  panText,
  toN,
  fromN,
  spec,
} from '../../fixtures/designData.ts';
import {
  faderFromDrag,
  panFromDrag,
  specFromDrag,
  eqParamFromDrag,
  initialWorld,
  resetEqParam,
} from '../../state/uiLogic.ts';

// Scale marks match the design's renderVals marks table exactly.
assert.deepEqual(
  SCALE_MARKS.map((m) => [m.pct, m.label]),
  [
    [100, '10'],
    [87.5, '5'],
    [75, '0'],
    [62.5, '-5'],
    [50, '-10'],
    [37.5, '-20'],
    [25, '-30'],
    [12.5, '-50'],
  ],
);
assert.equal(scaleTickColor('0'), 'var(--u4-text-1)');
assert.equal(scaleTickColor('-10'), '#566175');

// Fader anatomy per variant (template insets / cap sizes / readout).
assert.deepEqual(faderGeometry('strip').insets, { l: 0, r: 0, t: 26, b: 28 });
assert.deepEqual(faderGeometry('aside').insets, { l: 6, r: 6, t: 32, b: 34 });
assert.deepEqual(faderGeometry('master').insets, { l: 2, r: 2, t: 26, b: 28 });
assert.deepEqual(faderGeometry('strip').cap, {
  w: 36,
  h: 50,
  gripLR: 4,
  gripTB: 5,
  gripH: 14,
  left: '50%',
});
assert.deepEqual(faderGeometry('aside').cap, {
  w: 46,
  h: 60,
  gripLR: 5,
  gripTB: 6,
  gripH: 17,
  left: '50%',
});
assert.deepEqual(faderGeometry('master').cap, {
  w: 40,
  h: 52,
  gripLR: 4,
  gripTB: 5,
  gripH: 15,
  left: 'calc(50% - 4px)',
});
assert.equal(faderGeometry('master').railLeft, 'calc(50% - 6px)');
assert.equal(faderGeometry('strip').railLeft, 'calc(50% - 2px)');
assert.equal(faderGeometry('aside').readH, 18);
assert.equal(faderGeometry('strip').readH, 15);

// Pan marker: left = 50 + pan * 44.
assert.equal(panMarkerLeft(0), 50);
assert.equal(panMarkerLeft(0.28), 50 + 0.28 * 44);
assert.equal(panMarkerLeft(-1), 6);
assert.equal(panMarkerLeft(1), 94);
assert.equal(panText(0.45), 'R45');
assert.equal(panText(-0.28), 'L28');
assert.equal(panText(0), 'C');

// Fader drag math (design faderDown/move): dy up-positive, trackH - 60.
assert.equal(faderFromDrag(0.64, 0, 265, false), 0.64);
assert.ok(Math.abs(faderFromDrag(0.64, 26.5, 265, false) - 0.74) < 1e-9);
assert.ok(Math.abs(faderFromDrag(0.64, 26.5, 265, true) - (0.64 + 0.025)) < 1e-9);
assert.equal(faderFromDrag(0.99, 100, 100, false), 1);
assert.equal(faderFromDrag(0.01, -100, 100, false), 0);
// Unity reset target: dbToPos(0) = 0.75.
assert.equal(dbToPos(0), 0.75);
assert.equal(fmtDb(0.75), '0.0');

// Pan drag math with centre snap + 2dp rounding.
assert.equal(panFromDrag(0, 0, 100, false), 0);
assert.equal(panFromDrag(0, 50, 200, false), 0.5);
assert.equal(panFromDrag(0, 1, 200, false), 0); // |v| < 0.03 snaps to 0

// Spec drag math (input/dyn knobs): 180px full-scale, quartered in fine.
const gain = spec('gain');
assert.equal(toN(gain, 32), 32 / 60);
assert.equal(fromN(gain, 32 / 60), 32);
assert.equal(specFromDrag('gain', 0.5, 90, false), 60); // clamped + step .5
assert.equal(specFromDrag('gain', 0.5, 90, true), 37.5);

// EQ drag math delegates to the design's eqParamFromDrag.
assert.equal(eqParamFromDrag('g', 1, 0, 10, false), 1);
assert.equal(eqParamFromDrag('f', 1, 400, 30, false), 800);
assert.equal(eqParamFromDrag('f', 0, 80, 30, false), 160);
// Knob reset tables agree with uiLogic.resetEqParam.
for (const band of [1, 2, 3, 4]) {
  const w = resetEqParam(initialWorld(), 'in1', band, 'f');
  assert.equal(w.chans['in1'].eq!.bands[band - 1].f, EQ_RESET_F[band]);
  const wq = resetEqParam(initialWorld(), 'in1', band, 'q');
  assert.equal(wq.chans['in1'].eq!.bands[band - 1].q, EQ_RESET_Q[band]);
}
assert.equal(resetEqParam(initialWorld(), 'in1', 0, 'f').chans['in1'].eq!.hpf.f, 80);

// Ring geometry helpers (design knobItem dash/rot).
assert.equal(knobDash(0), '0.00 138.23');
assert.equal(knobDash(1), '103.67 138.23');
assert.equal(knobRot(0), 'rotate(-135.0 28 28)');
assert.equal(knobRot(1), 'rotate(135.0 28 28)');

console.log('signal.test.ts: all assertions passed');

// C2 EQ package: node tests for the pure view-models in eqModel.ts.
//
// Run: node --experimental-strip-types apps/web/src/ui/screens/channel/eq/eqModel.test.ts

import { strict as assert } from 'node:assert';
import {
  eqBandCardsVm,
  eqGraphVm,
  eqHpfCardVm,
  eqMasterToggle,
  eqNodeApply,
  eqPointFromPos,
  eqResponseAt,
  eqSelInfoText,
  eqTypeOptions,
  EQ_BAND_COLORS,
  type EqGraphVm,
} from './eqModel.ts';
import {
  filters,
  fmtF,
  fmtG,
  resp,
  type Eq,
} from '../../../fixtures/designData.ts';

/** Deterministic fixture EQ (mirrors the design's band defaults + edits). */
function fixtureEq(): Eq {
  return {
    on: true,
    hpf: { on: true, f: 94, slope: 12 },
    bands: [
      { type: 'LS', f: 120, g: 0.5, q: 0.7, on: true },
      { type: 'PEQ', f: 496, g: -5.5, q: 1.4, on: true },
      { type: 'PEQ', f: 2940, g: 0, q: 1.84, on: true },
      { type: 'HS', f: 8000, g: 0, q: 0.7, on: true },
    ],
  };
}

/** Parse an SVG path 'Mx y Lx y ...' into [x, y] pairs. */
function parsePath(d: string): Array<[number, number]> {
  assert.ok(d.startsWith('M'), 'path starts with M');
  return d
    .slice(1)
    .split(' L')
    .map(s => {
      const [x, y] = s.trim().split(' ').map(Number);
      assert.ok(Number.isFinite(x) && Number.isFinite(y), 'finite point: ' + s);
      return [x, y] as [number, number];
    });
}

const eq = fixtureEq();
const g: EqGraphVm = eqGraphVm(eq, 1);

// --- 1. summed curve matches the design's RBJ math at 10 sample points ---
{
  const pts = parsePath(g.curvePath);
  assert.equal(pts.length, 181, 'N=180 segments -> 181 points');
  const fl = filters(eq);
  for (let k = 0; k < 10; k++) {
    const i = Math.round((k * 180) / 9);
    const f = 20 * Math.pow(1000, i / 180);
    const expectY = Math.max(-20, Math.min(320, ((15 - resp(fl, f)) / 30) * 300));
    const [x, y] = pts[i];
    assert.ok(Math.abs(x - (i / 180) * 1000) < 0.06, `x[${i}] exact`);
    assert.ok(Math.abs(y - expectY) < 0.06, `y[${i}] f=${f.toFixed(0)} resp=${resp(fl, f).toFixed(2)}`);
  }
  assert.equal(eqResponseAt(eq, 1000), resp(fl, 1000), 'eqResponseAt reuses design math');
  assert.equal(eqResponseAt(null, 1000), 0, 'null eq is flat');
}

// --- 2. fill + strokes ---
{
  assert.equal(g.curveStroke, '#f0d36a');
  assert.equal(g.curveFill, 'rgba(240,211,106,0.10)');
  assert.ok(g.fillPath.endsWith(' L1000 150 L0 150 Z'), 'fill closes at the 0 dB line');
  assert.ok(g.bandFill.length > 0, 'selected band fill present');
  assert.equal(g.bandFillColor, EQ_BAND_COLORS[1] + '2e');
  assert.equal(g.bandStroke, EQ_BAND_COLORS[1]);
  const off = eqGraphVm({ ...eq, bands: eq.bands.map((b, i) => (i === 0 ? { ...b, on: false } : b)) }, 1);
  assert.equal(off.bandFill, '', 'band fill empty when the selected band is off');
  assert.equal(off.bandGhosts.length, 5, 'ghost per band even when off');
  assert.ok(off.bandGhosts[1].d.length > 0, 'off band keeps a non-empty grey ghost shape');
  assert.equal(off.bandGhosts[1].stroke, '#5a6272', 'off ghost strokes grey');
  assert.equal(off.bandGhosts[1].fill, 'rgba(90,98,114,0.14)', 'off ghost fills grey');
  assert.equal(off.bandGhosts[1].on, false, 'off ghost flagged Off');
  assert.equal(off.bandGhosts[2].on, true, 'sibling ghost stays On with band colour');
  assert.equal(off.bandGhosts[2].stroke, EQ_BAND_COLORS[2]);
  const bypassed = eqGraphVm({ ...eq, on: false }, 1);
  assert.equal(bypassed.curveStroke, '#5a6272');
  assert.equal(bypassed.bandFill, '', 'no band fill when EQ bypassed');
  assert.ok(
    bypassed.bandGhosts.every(gh => gh.d.length > 0),
    'bypassed EQ keeps all ghost shapes',
  );
  assert.ok(
    bypassed.bandGhosts.every(gh => gh.stroke === '#5a6272' && gh.fill === 'rgba(90,98,114,0.14)'),
    'bypassed ghosts all grey',
  );
}

// --- 3. grid + labels (design counts) ---
{
  assert.equal(g.gridV.length, 28, 'decade minors in 20..20000');
  assert.equal(g.gridH.length, 5);
  assert.deepEqual(
    g.gridH.map(h => h.y),
    ['30.0', '90.0', '150.0', '210.0', '270.0'],
  );
  assert.equal(g.gridH[2].stroke, 'var(--u4-graph-unity)', '0 dB line emphasized');
  assert.deepEqual(
    g.freqLabels.map(l => l.label),
    ['50', '100', '200', '500', '1k', '2k', '5k', '10k'],
  );
  assert.deepEqual(
    g.gainLabels.map(l => l.label),
    ['+12', '+6', '0', '-6', '-12'],
  );
}

// --- 4. nodes ---
{
  assert.equal(g.nodes.length, 5);
  const [hpf, low] = g.nodes;
  assert.equal(hpf.label, 'H');
  assert.equal(hpf.size, 30, 'unselected node is 30px');
  assert.equal(low.label, '1');
  assert.equal(low.size, 36, 'selected node is 36px');
  assert.equal(low.ring, '#ffffff');
  assert.equal(hpf.ring, 'rgba(0,0,0,.55)');
  assert.equal(low.color, EQ_BAND_COLORS[1]);
  assert.equal(low.x, ((Math.log(120 / 20) / Math.log(1000)) * 100).toFixed(2));
  assert.equal(low.y, (((15 - 0.5) / 30) * 100).toFixed(2));
  assert.equal(hpf.y, '60.00', 'HPF node rides the -3 dB line');
  assert.equal(low.aria, 'Low node, 120 Hz, +0.5 dB');
  const dim = eqGraphVm({ ...eq, on: false }, 1);
  assert.equal(dim.nodes[1].op, 0.4, 'nodes dim when EQ bypassed');
}

// --- 5. graph drag math + HPF clamp (design node branch) ---
{
  assert.deepEqual(eqPointFromPos(0, 0), { f: 20, g: 15 });
  assert.deepEqual(eqPointFromPos(1, 1), { f: 20000, g: -15 });
  const mid = eqPointFromPos(0.5, 0.5);
  assert.ok(Math.abs(mid.f - 632.4555) < 0.01 && mid.g === 0);
  assert.deepEqual(eqPointFromPos(-1, 2), { f: 20, g: -15 }, 'clamped to the box');
  assert.deepEqual(eqNodeApply(0, 5000, 9), { f: 1000, g: -3 }, 'HPF clamps at 1000 Hz, gain fixed');
  assert.deepEqual(eqNodeApply(0, 40, -12), { f: 40, g: -3 });
  assert.deepEqual(eqNodeApply(2, 30000, -20), { f: 20000, g: -15 });
  assert.deepEqual(eqNodeApply(2, 400, 1.05), { f: 400, g: 1.1 }, 'gain rounds to 0.1');
}

// --- 6. summary + master toggle texts ---
{
  assert.equal(eqSelInfoText(eq, 1), 'Low · 120 Hz · +0.5 dB · Q 0.70');
  assert.equal(eqSelInfoText(eq, 0), 'HPF · 94 Hz · 12 dB/oct');
  assert.equal(
    eqSelInfoText({ ...eq, hpf: { ...eq.hpf, on: false } }, 0),
    'HPF · 94 Hz · 12 dB/oct · off',
  );
  const off = { ...eq, bands: eq.bands.map((b, i) => (i === 2 ? { ...b, on: false } : b)) };
  assert.ok(eqSelInfoText(off, 3).endsWith('· off'));
  assert.equal(eqMasterToggle(eq).label, 'EQ In');
  assert.equal(eqMasterToggle(eq).bg, '#5cdb5c');
  assert.equal(eqMasterToggle({ ...eq, on: false }).label, 'EQ Bypassed');
}

// --- 7. shelf/bell type sets per band ---
{
  assert.deepEqual(eqTypeOptions(0, 'LS').values, ['LS', 'PEQ']);
  assert.deepEqual(
    eqTypeOptions(0, 'LS').options.map(o => o.label),
    ['Shelf', 'Bell'],
  );
  assert.deepEqual(eqTypeOptions(1, 'PEQ').values, ['PEQ']);
  assert.deepEqual(eqTypeOptions(2, 'PEQ').values, ['PEQ']);
  assert.deepEqual(eqTypeOptions(3, 'HS').values, ['PEQ', 'HS']);
  assert.equal(eqTypeOptions(0, 'LS').options[0].bg, 'var(--u4-text-1)', 'active chip is lit');
  assert.equal(eqTypeOptions(0, 'LS').options[1].bg, 'var(--u4-btn-bg)');
}

// --- 8. cards ---
{
  const hpf = eqHpfCardVm(eq, 0);
  assert.equal(hpf.slope, 12);
  assert.equal(hpf.freq, 94);
  assert.equal(hpf.border, EQ_BAND_COLORS[0], 'selected HPF card takes the accent border');
  assert.equal(hpf.onLabel, 'On');
  const cards = eqBandCardsVm(eq, 1);
  assert.equal(cards.length, 4);
  assert.equal(cards[0].name, 'Low');
  assert.equal(cards[0].border, EQ_BAND_COLORS[1], 'selected card takes the accent border');
  assert.equal(cards[1].border, 'var(--u4-line-panel)');
  assert.equal(cards[1].types.length, 1, 'Lo-Mid offers Bell only');
  assert.equal(cards[3].types.length, 2, 'High offers Bell/Shelf');
  assert.deepEqual([cards[2].freq, cards[2].gain, cards[2].q], [2940, 0, 1.84]);
  assert.equal(fmtF(cards[2].freq), '2.94 kHz', 'card value formatting');
  assert.equal(fmtG(cards[0].gain), '+0.5 dB');
}

console.log('eqModel.test.ts: all assertions passed');

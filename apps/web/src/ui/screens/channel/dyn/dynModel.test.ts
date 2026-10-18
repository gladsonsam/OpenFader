// C3 Gate+Compressor package: node tests for the pure dyn geometry.
// Run: node --experimental-strip-types apps/web/src/ui/screens/channel/dyn/dynModel.test.ts

import { strict as assert } from 'node:assert';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { clamp, compOut, gateOut, type Comp, type Gate } from '../../../fixtures/designData.ts';
import { validateLayout } from '../../../modules/validate.ts';
import {
  DYN_KNOBS,
  dynAria,
  dynAxis,
  dynCurvePath,
  dynDot,
  dynFillColor,
  dynFillPath,
  dynGr,
  dynGrid,
  dynMap,
  dynSummary,
  dynThreshold,
  dynToggle,
  type DynKind,
} from './dynModel.ts';

const HERE = dirname(fileURLToPath(import.meta.url));

let n = 0;
function ok(cond: unknown, msg: string): void {
  n++;
  assert(cond, msg);
}

const COMP: Comp = { on: true, thr: -13, ratio: 3, att: 10, rel: 120, knee: 3, makeup: 0 };
const GATE: Gate = { on: true, thr: -47, range: 40, att: 0.5, hold: 50, rel: 150 };

// --- transfer curves: 10 sample points each vs designData math ---------------
// Expected segment for x: map(x) + ' ' + (200 - clamp(map(fOut(x)), -4, 204)).

function seg(kind: DynKind, p: Comp | Gate, x: number, lo: number): string {
  const f = kind === 'gate' ? gateOut(p as Gate, x) : compOut(p as Comp, x);
  return dynMap(lo, x).toFixed(1) + ' ' + (200 - clamp(dynMap(lo, f), -4, 204)).toFixed(1);
}

function pathSegs(kind: DynKind, p: Comp | Gate): string[] {
  return dynCurvePath(kind, p).slice(1).split(' L');
}

for (const x of [-60, -52.5, -45, -37.5, -30, -22.5, -15, -7.5, -3.75, 0]) {
  const segs = pathSegs('comp', COMP);
  ok(segs.includes(seg('comp', COMP, x, -60)), 'comp curve @ ' + x + ' dB');
}
// Soft knee: just below thr the curve is already bending (y != x and y != hard line).
{
  const yKnee = compOut(COMP, -14);
  ok(yKnee < -14 && yKnee > -14.5, 'comp soft knee bends below thr: ' + yKnee);
  // (-14 dB is not one of the 81 curve samples; the sampled knee is covered above.)
}
for (const x of [-80, -70, -60, -50, -47, -40, -30, -20, -10, 0]) {
  const segs = pathSegs('gate', GATE);
  ok(segs.includes(seg('gate', GATE, x, -80)), 'gate curve @ ' + x + ' dB');
}

// --- hand-computed literals (from design-logic.js.txt gateOut/compOut) -------
{
  const segs = pathSegs('comp', COMP);
  ok(segs[0] === '0.0 200.0', 'comp starts bottom-left, got ' + segs[0]);
  ok(segs[40] === '100.0 100.0', 'comp unity @ -30 dB, got ' + segs[40]);
  ok(segs[80] === '200.0 28.9', 'comp @ 0 dB -> -8.7 out, got ' + segs[80]);
  const gsegs = pathSegs('gate', GATE);
  ok(gsegs[0] === '0.0 204.0', 'gate clamps below plot @ -80, got ' + gsegs[0]);
  ok(gsegs.includes('82.5 117.5'), 'gate knee point @ thr -47');
  ok(gsegs[80] === '200.0 0.0', 'gate unity @ 0 dB');
  const off = pathSegs('comp', { ...COMP, on: false });
  ok(off[40] === '100.0 100.0' && off[80] === '200.0 0.0', 'bypassed comp is unity');
  ok(dynFillPath('comp', COMP).endsWith(' L200 200 L0 200 Z'), 'fill closes to bottom');
  ok(dynFillColor('comp') === '#ff9f431c' && dynFillColor('gate') === '#3ed0e01c', 'fill colours');
}

// --- threshold tag / line ----------------------------------------------------
{
  const t = dynThreshold(COMP, -60);
  ok(t.x === '156.7' && t.leftPct === '78.3' && t.text === '-13.0 dB', 'comp threshold ' + JSON.stringify(t));
  const g = dynThreshold(GATE, -80);
  ok(g.x === '82.5' && g.leftPct === '41.3' && g.text === '-47.0 dB', 'gate threshold ' + JSON.stringify(g));
}

// --- operating dot -----------------------------------------------------------
{
  const d = dynDot('comp', COMP, -10);
  ok(d.leftPct === '83.3' && d.topPct === '20.0', 'comp dot @ -10 dB ' + JSON.stringify(d));
  const s = dynDot('comp', COMP, -90);
  ok(s.leftPct === '0.0' && s.topPct === '100.0', 'comp dot silent bottom-left');
  const gs = dynDot('gate', GATE, -90);
  ok(gs.leftPct === '0.0' && gs.topPct === '100.0', 'gate dot silent bottom-left');
}

// --- GR readout --------------------------------------------------------------
{
  const r = dynGr('comp', COMP, -10);
  ok(r.pct === '8.3' && r.text === '-2.0', 'comp GR @ -10 dB ' + JSON.stringify(r));
  ok(dynGr('comp', COMP, -90).text === '0.0', 'comp GR silent');
  ok(dynGr('comp', { ...COMP, on: false }, -10).text === '0.0', 'comp GR bypassed');
  const g = dynGr('gate', GATE, -60);
  ok(g.pct === '50.0' && g.text === '-40.0', 'gate GR below thr ' + JSON.stringify(g));
  ok(dynGr('gate', GATE, -10).text === '0.0', 'gate GR above thr');
}

// --- grid + axes -------------------------------------------------------------
{
  const grid = dynGrid(-60);
  ok(grid.length === 5 && grid[0].p === '33.3' && grid[0].q === '166.7', 'comp grid 5 lines');
  ok(dynGrid(-80).length === 7, 'gate grid 7 lines');
  const ax = dynAxis('comp');
  ok(ax.length === 5 && ax[0].label === '-60' && ax[4].label === '0', 'comp axis labels');
  ok(ax[0].left === '0.0' && ax[0].top === '100.0' && ax[0].shift === '0', 'comp axis first tick');
  ok(ax[4].left === '100.0' && ax[4].top === '0.0' && ax[4].shift === '-100%', 'comp axis last tick');
  ok(ax[2].shift === '-50%', 'comp axis mid tick');
  const gx = dynAxis('gate');
  ok(gx.length === 5 && gx[0].label === '-80' && gx[4].label === '0', 'gate axis labels');
}

// --- toggle / summary / aria / knobs -----------------------------------------
{
  const on = dynToggle('comp', true);
  ok(on.bg === '#5cdb5c' && on.fg === '#10240f' && on.label === 'Compressor In' && on.pressed === 'true', 'comp toggle in');
  const off = dynToggle('gate', false);
  ok(off.bg === '#2b3038' && off.label === 'Gate Bypassed', 'gate toggle bypassed');
  ok(dynSummary('comp', GATE, COMP) === 'Thr -13.0 dB · 3.0:1 · Makeup 0.0 dB', 'comp summary');
  ok(dynSummary('gate', GATE, COMP) === 'Thr -47.0 dB · Range 40 dB', 'gate summary');
  ok(dynAria('comp') === 'Compressor transfer curve' && dynAria('gate') === 'Gate transfer curve', 'aria');
  ok(DYN_KNOBS.comp.map(k => k.prop).join(',') === 'thr,ratio,knee,att,rel,makeup', 'comp knob order');
  ok(DYN_KNOBS.gate.map(k => k.prop).join(',') === 'thr,range,att,hold,rel', 'gate knob order');
  ok(DYN_KNOBS.comp[0].specKey === 'cthr' && DYN_KNOBS.comp[0].defaultValue === -20, 'comp thr spec');
  ok(DYN_KNOBS.gate[0].bind === 'param:gate_thr', 'gate thr bind');
}

// --- default layouts validate against the L0 schema --------------------------
{
  for (const f of ['gate.default.json', 'comp.default.json']) {
    const tree = JSON.parse(readFileSync(join(HERE, '..', '..', '..', 'layouts', f), 'utf8'));
    const v = validateLayout(tree);
    ok(v.ok, f + ' validates: ' + JSON.stringify(v.errors));
  }
  const comp = JSON.parse(readFileSync(join(HERE, '..', '..', '..', 'layouts', 'comp.default.json'), 'utf8'));
  ok(comp.children.map((c: { id: string }) => c.id).join(',') === 'head,body,dyn-empty', 'comp layout rows');
  ok(comp.children[0].module === 'dyn-summary' && comp.children[1].module === 'row', 'comp modules');
  ok(comp.children[1].children.map((c: { id: string }) => c.id).join(',') === 'graph,knobs', 'comp body');
  const gate = JSON.parse(readFileSync(join(HERE, '..', '..', '..', 'layouts', 'gate.default.json'), 'utf8'));
  ok(gate.children[1].children[0].module === 'dyn-transfer-graph', 'gate graph module');
  ok(gate.children[1].children[1].module === 'dyn-knob-panel', 'gate knob module');
}

console.log(`dynModel.test.ts: ${n} assertions passed`);

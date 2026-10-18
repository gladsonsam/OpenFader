// W5b channel scope tests: buildChannelScope derives every layout binding
// from the world (pure, node-testable).
// Run: node --experimental-strip-types apps/web/src/ui/screens/channel/channelScope.test.ts
import { strict as assert } from 'node:assert';
import {
  initialWorld,
  selectChannel,
  setTab,
  type World,
} from '../../state/uiLogic.ts';
import { CHANNEL_TAB_ORDER } from './channelModel.ts';
import { DYN_KNOBS } from './dyn/dynModel.ts';
import { buildChannelScope, dynKindForTab } from './channelScope.ts';

function scopeAt(sel: string, tab: 'input' | 'eq' | 'gate' | 'comp' | 'sends') {
  let w: World = selectChannel(initialWorld(), sel);
  w = setTab(w, tab);
  const ch = w.chans[sel];
  return buildChannelScope({
    state: w.state,
    chans: w.chans,
    fx: w.fx,
    sel: ch,
    frozen: true,
    stripMeter: { getLevel: () => -90 },
    hbarMeter: { getLevel: () => -90 },
    levelDb: -90,
    dynGetLevel: null,
  });
}

assert.equal(dynKindForTab('gate'), 'gate');
assert.equal(dynKindForTab('comp'), 'comp');
assert.equal(dynKindForTab('eq'), 'comp', 'only gate renders the gate tab');

// --- tab rail: data-driven, selection follows the store tab ---
{
  const s = scopeAt('in2', 'eq');
  assert.equal(s.selId, 'in2');
  assert.deepEqual(s.tabs.map(t => t.id), CHANNEL_TAB_ORDER);
  assert.ok(s.tabs[1].current && !s.tabs[0].current, 'eq selected');
  assert.deepEqual(s.lists.tabs, s.tabs, 'repeat list is the descriptor list');
}

// --- strip + input + sends vms ---
{
  const s = scopeAt('in2', 'input');
  assert.equal(s.master.name, 'Ch 2', 'master strip vm, got ' + s.master.name);
  assert.ok(s.inputVm && s.inputVm.hasInput, 'input vm present');
  assert.equal(s.levelText, '-∞ dBFS', 'silent level text, got ' + s.levelText);
  assert.ok(s.sendRows.length > 0, 'send rows present');
  assert.deepEqual(s.lists.sends, s.sendRows);
  assert.equal(s.stripMeter != null && s.hbarMeter != null, true, 'meter feeds pass through');
}

// --- eq scope: graph, cards, hpf, summary ---
{
  const s = scopeAt('in2', 'eq');
  assert.ok(s.eqGraph && s.eqGraph.curvePath.length > 0, 'curve path present');
  assert.ok(s.eqGraph.nodes.length === 5, 'hpf + 4 nodes, got ' + s.eqGraph.nodes.length);
  assert.equal(s.eqBands.length, 4, 'four band cards');
  assert.deepEqual(s.lists.eqBands, s.eqBands);
  assert.ok(s.eqHpf && s.eqHpf.slope === 12, 'hpf card vm');
  assert.ok(s.eqSummary.length > 0, 'summary text: ' + s.eqSummary);
  assert.ok(s.eqMaster && s.eqMaster.label.length > 0, 'master toggle visuals');
}

// --- dyn scope: kind, toggle, curve, knobs from the descriptor list ---
for (const tab of ['gate', 'comp'] as const) {
  const s = scopeAt('in2', tab);
  const kind = tab;
  assert.equal(s.dynKind, kind);
  assert.equal(s.dynKnobs, DYN_KNOBS[kind], 'knobs come from the descriptor list');
  assert.ok(s.dynPath.startsWith('M'), 'transfer curve path');
  assert.ok(s.dynGrid.length > 0 && s.dynAxis.length > 0, 'grid + axis');
  assert.ok(s.dynThrText.endsWith('dB'), 'threshold tag: ' + s.dynThrText);
  assert.ok(s.dynSummary.length > 0, 'dyn summary: ' + s.dynSummary);
  assert.ok(s.dynLabel.length > 0 && s.dynPressed.length > 0, 'toggle visuals');
  assert.ok(s.dynParams != null && s.dynGate != null && s.dynComp != null, 'live params');
}

console.log('channel channelScope.test.ts: ok');

// S78 Routing view-model tests (against design-logic.js.txt routingVals).
// Run: node --experimental-strip-types apps/web/src/ui/screens/routing/routingVm.test.ts

import { strict as assert } from 'node:assert';
import { initialWorld, patchSrc, setPatchGroup, setPatchSel, setRouteBank } from '../../state/uiLogic.ts';
import { buildRoutingVm } from './routingVm.ts';

const w0 = initialWorld();
{
  const vm = buildRoutingVm(w0.state, w0.chans);
  assert.deepEqual(vm.banks.map(b => b.label), ['1–16', '17–32', '33–48']);
  assert.deepEqual(vm.banks.map(b => b.pressed), [true, false, false]);
  assert.equal(vm.banks[0].bg, '#e8edf5');
  assert.equal(vm.banks[1].bg, '#2b3038');
  assert.equal(vm.tiles.length, 16);
  assert.equal(vm.tiles[0].name, 'Ch 1');
  assert.equal(vm.tiles[0].src, 'Lcl 01');
  assert.equal(vm.tiles[0].pressed, true);
  assert.equal(vm.tiles[0].border, '#ffffff');
  assert.equal(vm.tiles[0].bg, '#1b2330');
  assert.equal(vm.tiles[1].pressed, false);
  assert.equal(vm.tiles[1].border, '#222a36');
  assert.equal(vm.tiles[0].srcColor, '#5fe3f0');
  assert.equal(vm.selName, 'Ch 1');
  assert.deepEqual(vm.groups.map(g => g.label), ['Local', 'Network', 'USB']);
  assert.deepEqual(vm.groups.map(g => g.pressed), [true, false, false]);
  assert.equal(vm.srcTiles.length, 32);
  assert.equal(vm.srcTiles[0].label, 'Lcl 01');
  // Lcl 01 is patched to Ch 1 (the selection): green + 'patched'.
  assert.equal(vm.srcTiles[0].used, 'patched');
  assert.equal(vm.srcTiles[0].pressed, true);
  assert.equal(vm.srcTiles[0].bg, '#5cdb5c');
  assert.equal(vm.srcTiles[0].fg, '#10240f');
  assert.equal(vm.srcTiles[0].border, '#5cdb5c');
  // Lcl 02 is used by Ch 2: dark tile with an arrow sub-label.
  assert.equal(vm.srcTiles[1].used, '→ Ch 2');
  assert.equal(vm.srcTiles[1].bg, '#141b26');
  assert.equal(vm.srcTiles[1].fg, '#c9d1dc');
}

// Route bank switch changes the tile window.
{
  const w = setRouteBank(w0, 1);
  const vm = buildRoutingVm(w.state, w.chans);
  assert.equal(vm.tiles[0].name, 'Ch 17');
  assert.deepEqual(vm.banks.map(b => b.pressed), [false, true, false]);
  // patchSel still in1: no tile selected in this bank.
  assert.ok(vm.tiles.every(t => !t.pressed));
}

// Patch-group switch relabels the source grid (Network bank).
{
  const w = setPatchGroup(w0, 'Net');
  const vm = buildRoutingVm(w.state, w.chans);
  assert.equal(vm.srcTiles[0].label, 'Net 01');
  // Ch 33..48 default to Net 01..16, so Net 01 is in use by Ch 33.
  assert.equal(vm.srcTiles[0].used, '→ Ch 33');
  assert.equal(vm.srcTiles[0].bg, '#141b26');
}

// Patching advances the selection (uiLogic.nextPatchSel port).
{
  const w = patchSrc(w0, 'Lcl 02');
  assert.equal(w.chans['in1'].src, 'Lcl 02');
  assert.equal(w.state.patchSel, 'in2');
  const vm = buildRoutingVm(w.state, w.chans);
  assert.equal(vm.selName, 'Ch 2');
  assert.equal(vm.tiles[1].pressed, true);
}

// Selecting another channel moves the white border.
{
  const w = setPatchSel(w0, 'in5');
  const vm = buildRoutingVm(w.state, w.chans);
  assert.equal(vm.tiles[4].pressed, true);
  assert.equal(vm.tiles[0].pressed, false);
  assert.equal(vm.selName, 'Ch 5');
}

console.log('routingVm.test.ts: ok');

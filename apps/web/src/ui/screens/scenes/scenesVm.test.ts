// S910 Scenes view-model tests (against design-logic.js.txt sceneCards/recall).
// Run: node --experimental-strip-types apps/web/src/ui/screens/scenes/scenesVm.test.ts

import { strict as assert } from 'node:assert';
import { initialWorld, pickScene, recallScene, setSceneConfirm } from '../../state/uiLogic.ts';
import { buildScenesVm } from './scenesVm.ts';

const w0 = initialWorld(); // scene 4 ('Changeover'), scenePv 4, confirm mode

// --- default: no pending, card 5 current + selected ---
{
  const vm = buildScenesVm(w0.state);
  assert.equal(vm.cards.length, 10);
  assert.deepEqual(vm.cards.map(c => c.num), ['001', '002', '003', '004', '005', '006', '007', '008', '009', '010']);
  assert.equal(vm.cards[4].name, 'Changeover');
  assert.equal(vm.cards[0].name, 'Soundcheck');
  assert.equal(vm.cards[9].name, 'Walk-out');
  assert.deepEqual(vm.cards.map(c => c.tag), ['', '', '', '', 'Current', '', '', '', '', '']);
  assert.deepEqual(vm.cards.map(c => c.pressed), [false, false, false, false, true, false, false, false, false, false]);
  assert.equal(vm.cards[4].border, '#7be27b');
  assert.equal(vm.cards[4].bg, '#1b2330');
  assert.equal(vm.cards[0].border, 'var(--u4-line-faint)');
  assert.equal(vm.cards[0].bg, 'var(--u4-panel-bg)');
  assert.equal(vm.recall.pending, false);
  assert.equal(vm.recall.tag, 'Current scene');
  assert.equal(vm.recall.tagColor, '#7be27b');
  assert.equal(vm.recall.num, '005');
  assert.equal(vm.recall.name, 'Changeover');
  assert.equal(vm.recall.barBorder, 'var(--u4-line-faint)');
  assert.equal(vm.recall.border, 'var(--u4-divider)');
  assert.equal(vm.recall.bg, '#161c26');
  assert.equal(vm.recall.text, 'var(--u4-text-dim)');
  assert.equal(vm.recall.disabled, true);
}

// --- confirm mode: picking previews (pending, amber), recall loads ---
{
  const w = pickScene(w0, 7); // 'Set 2'
  const vm = buildScenesVm(w.state);
  assert.equal(vm.recall.pending, true);
  assert.equal(vm.recall.tag, 'Selected · not recalled yet');
  assert.equal(vm.recall.tagColor, '#f0b44a');
  assert.equal(vm.recall.num, '008');
  assert.equal(vm.recall.name, 'Set 2');
  assert.equal(vm.recall.barBorder, '#a87622');
  assert.equal(vm.recall.border, '#f0b44a');
  assert.equal(vm.recall.bg, '#f0b44a');
  assert.equal(vm.recall.text, '#241703');
  assert.equal(vm.recall.disabled, false);
  // Selected card: amber border; current card keeps its tag but loses pressed.
  assert.equal(vm.cards[7].border, '#f0b44a');
  assert.equal(vm.cards[7].bg, '#1b2330');
  assert.deepEqual(vm.cards.map(c => c.pressed), [false, false, false, false, false, false, false, true, false, false]);
  assert.equal(vm.cards[4].tag, 'Current');
  assert.equal(vm.cards[7].tag, '');
  const r = recallScene(w);
  assert.equal(r.state.scene, 7);
  assert.equal(r.state.mode, 'mixer');
  const vm2 = buildScenesVm(r.state);
  assert.equal(vm2.recall.pending, false);
  assert.equal(vm2.cards[7].tag, 'Current');
  assert.equal(vm2.cards[7].border, '#7be27b');
}

// --- instant mode: picking loads immediately ---
{
  const w = setSceneConfirm(w0, false);
  const p = pickScene(w, 1);
  assert.equal(p.state.scene, 1);
  assert.equal(p.state.scenePv, 1);
  const vm = buildScenesVm(p.state);
  assert.equal(vm.recall.pending, false);
  assert.equal(vm.cards[1].tag, 'Current');
  assert.equal(vm.cards[4].tag, '');
}

// --- out-of-range picks are ignored ---
{
  const a = pickScene(w0, -1);
  const b = pickScene(w0, 10);
  assert.equal(a.state.scenePv, w0.state.scenePv);
  assert.equal(b.state.scenePv, w0.state.scenePv);
}

console.log('scenesVm.test.ts: ok');

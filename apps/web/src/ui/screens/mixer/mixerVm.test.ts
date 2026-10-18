// M1 Mixer view-model tests (against design-logic.js.txt stripVals/renderVals).
// Run: node --experimental-strip-types apps/web/src/ui/screens/mixer/mixerVm.test.ts

import { strict as assert } from 'node:assert';
import { initialWorld, setBank, setMix, setSof, toggleMute } from '../../state/uiLogic.ts';
import { masterVm, mixerCells, mixerMasterCell, mixerSlots, SLOT_COUNT, stripsPerScreen, stripVm } from './mixerVm.ts';
import { fmtDb } from '../../fixtures/designData.ts';

const w0 = initialWorld();

// --- slots: always 16 cells, bank items then empties ---
{
  assert.equal(mixerSlots('ch1').length, SLOT_COUNT);
  assert.deepEqual(mixerSlots('ch1').slice(0, 3), ['in1', 'in2', 'in3']);
  assert.ok(mixerSlots('ch1').every(s => s !== ''));
  const mtx = mixerSlots('mtx');
  assert.deepEqual(mtx.slice(0, 3), ['mtx1', 'mtx2', 'mtx3']);
  assert.equal(mtx[8], 'mainlr');
  assert.equal(mtx[9], 'mainc');
  assert.ok(mtx.slice(10).every(s => s === ''), 'mtx has 6 empty placeholders');
  assert.deepEqual(mixerSlots('dca').slice(0, 2), ['dca1', 'dca2']);
}

// --- ch strip: full anatomy, selection ring on sel ---
{
  const vm = stripVm(w0.state, w0.chans, 'in1')!;
  assert.equal(vm.name, 'Ch 1');
  assert.equal(vm.sub, 'Lcl 01');
  assert.equal(vm.color, '#d64545');
  assert.equal(vm.ring, 'inset 0 0 0 2px #d64545');
  assert.equal(vm.pan, w0.chans['in1'].pan);
  assert.equal(vm.panLeft, 50 + (w0.chans['in1'].pan ?? 0) * 44);
  assert.equal(vm.hasEq, true);
  assert.ok(vm.eqPath.startsWith('M'));
  assert.equal(vm.eqStroke, '#d9c24a');
  assert.ok(vm.dynPath.startsWith('M'));
  assert.equal(vm.eqAria, 'Open EQ for Ch 1');
  assert.equal(vm.dynAria, 'Open dynamics for Ch 1');
  assert.equal(vm.selectAria, 'Select Ch 1, source Lcl 01');
  assert.equal(vm.muted, false);
  assert.equal(vm.muteEnabled, true);
  assert.equal(vm.useSend, false);
  assert.equal(vm.value, w0.chans['in1'].fader);
  assert.equal(vm.readout, fmtDb(w0.chans['in1'].fader));
  assert.equal(vm.readColor, '#c9d1dc');
  assert.equal(vm.faderAria, 'Fader Ch 1 ' + fmtDb(w0.chans['in1'].fader) + ' dB');
  const other = stripVm(w0.state, w0.chans, 'in2')!;
  assert.equal(other.ring, 'none');
}

// --- DCA: no pan, no EQ/Dyn panes (collapsed placeholders) ---
{
  const vm = stripVm(w0.state, w0.chans, 'dca1')!;
  assert.equal(vm.pan, null);
  assert.equal(vm.hasEq, false);
  assert.equal(vm.eqPath, '');
  assert.equal(vm.dynPath, '');
  assert.equal(vm.color, '#dc2626');
}

// --- SOF: strips show the send, cyan; master becomes the target mix ---
{
  const w = setMix(setSof(w0, true), 0);
  const vm = stripVm(w.state, w.chans, 'in1')!;
  assert.equal(vm.useSend, true);
  assert.equal(vm.value, w.chans['in1'].sends![0]);
  assert.equal(vm.readColor, '#22d3e6');
  assert.ok(vm.faderAria.startsWith('Send to Mix 1 from Ch 1 '));
  const dca = stripVm(w.state, w.chans, 'dca1')!;
  assert.equal(dca.useSend, false, 'DCA has no sends: shows fader');
  const m = masterVm(w.state, w.chans)!;
  assert.equal(m.id, 'mix1');
  assert.equal(m.name, 'Mix 1');
  assert.equal(m.sub, 'Aux bus');
  assert.equal(m.tag, 'SENDS MASTER');
  assert.equal(m.tagColor, '#5fe3f0');
  assert.equal(m.kind, 'sof');
  assert.equal(m.readColor, '#e8edf5', 'master readout is always light');
}

// --- master (no SOF): Main LR, stereo out, gold ---
{
  const m = masterVm(w0.state, w0.chans)!;
  assert.equal(m.id, 'mainlr');
  assert.equal(m.name, 'Main LR');
  assert.equal(m.sub, 'Stereo out');
  assert.equal(m.tag, 'MASTER');
  assert.equal(m.tagColor, '#ffd25a');
  assert.equal(m.kind, 'master');
  assert.equal(m.value, w0.chans['mainlr'].fader);
}

// --- mute + lock gate flow through the vm ---
{
  const w = toggleMute(w0, 'in1');
  assert.equal(stripVm(w.state, w.chans, 'in1')!.muted, true);
  assert.equal(stripVm(w.state, w.chans, 'in1')!.muteEnabled, true);
}

// --- spp default ---
{
  assert.equal(stripsPerScreen(null), 12);
  assert.equal(stripsPerScreen(8), 8);
}

// --- unknown id ---
{
  assert.equal(stripVm(w0.state, w0.chans, 'nope'), null);
}

// --- layout scope: filled cells in slot order + master cell, meters attached ---
{
  const cells = mixerCells(w0.state, w0.chans, null, true);
  assert.equal(cells.length, 16, 'ch1 bank fills all 16 slots');
  assert.deepEqual(cells.map(c => c.id).slice(0, 3), ['in1', 'in2', 'in3']);
  assert.ok(cells.every(c => typeof c.meter.getLevel === 'function'));
  // frozen feeds report silence (the design reference shows silent meters).
  assert.equal(cells[0].meter.getLevel(0), -90);
  assert.equal(cells[0].meter.getPeak?.(0) ?? null, null);
  assert.equal(cells[0].meter.getClip?.(0) ?? true, false);
  const m = mixerMasterCell(w0.state, w0.chans, null, true)!;
  assert.equal(m.id, 'mainlr');
  assert.equal(m.meter.getLevel(0), -90);
  assert.equal(m.meter.getLevel(1), -90);
}

void setBank;
console.log('mixerVm.test: ok');

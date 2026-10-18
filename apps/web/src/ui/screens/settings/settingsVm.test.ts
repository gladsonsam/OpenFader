// S910 Settings view-model tests (against design-logic.js.txt settingsVals).
// Run: node --experimental-strip-types apps/web/src/ui/screens/settings/settingsVm.test.ts

import { strict as assert } from 'node:assert';
import { initialWorld, setFine, setLocked, setMix, setMuteEnable, setPeakHold, setSceneConfirm, setSof, setSpp } from '../../state/uiLogic.ts';
import {
  buildSettingsVm,
  consoleDescription,
  SETTING_DESCRIPTORS,
  validateSettingDescriptors,
  type SettingDescriptor,
} from './settingsVm.ts';

const w0 = initialWorld();

// --- descriptor list validates; rejects bad lists ---
{
  assert.deepEqual(validateSettingDescriptors(SETTING_DESCRIPTORS), []);
  assert.deepEqual(SETTING_DESCRIPTORS.map(d => d.panel), [
    'Display', 'Display', 'Faders', 'Faders', 'Faders', 'Safety', 'Safety', 'Safety', 'About', 'About',
  ]);
  assert.ok(SETTING_DESCRIPTORS.every(d => d.bind.startsWith('setting:')));
  const dup = [...SETTING_DESCRIPTORS, SETTING_DESCRIPTORS[0]];
  assert.ok(validateSettingDescriptors(dup).some(e => e.includes('duplicated')));
  assert.ok(validateSettingDescriptors([]).length > 0);
  const bad = [{ ...SETTING_DESCRIPTORS[0], control: 'knob' } as unknown as SettingDescriptor];
  assert.ok(validateSettingDescriptors(bad).some(e => e.includes('control')));
  const badBind = [{ ...SETTING_DESCRIPTORS[0], bind: 'param:fader' }];
  assert.ok(validateSettingDescriptors(badBind).some(e => e.includes('bind')));
}

// --- sections/rows mirror the design ---
{
  const vm = buildSettingsVm(w0.state);
  assert.deepEqual(vm.map(s => s.title), ['Display', 'Faders', 'Safety', 'About']);
  assert.deepEqual(vm[0].rows.map(r => r.label), ['Strips per screen', 'Meter peak hold']);
  assert.deepEqual(vm[1].rows.map(r => r.label), ['Fine control', 'Sends on faders', 'Send target']);
  assert.deepEqual(vm[2].rows.map(r => r.label), ['Mute Enable', 'Screen lock', 'Scene recall']);
  assert.deepEqual(vm[3].rows.map(r => r.label), ['App version', 'Console']);
  assert.equal(vm[0].rows[0].desc, 'Fewer strips are wider and easier to hit. Swipe to reach the rest.');
  assert.equal(vm[0].rows[1].desc, 'Keeps a line at the highest level for 1.5 s.');
  assert.equal(vm[1].rows[0].desc, 'Faders and knobs move a quarter as far per drag.');
  assert.equal(vm[1].rows[1].desc, 'Strip faders set sends to the target mix. The master strip shows that mix.');
  assert.equal(vm[1].rows[2].desc, 'Which mix the faders control when sends are on.');
  assert.equal(vm[2].rows[0].desc, 'When off, mute buttons ignore taps.');
  assert.equal(vm[2].rows[1].desc, 'Blocks faders, knobs, mutes and patching. Unlock from the top bar.');
  assert.equal(vm[2].rows[2].desc, 'Confirm needs a press on RECALL. Instant loads on tap.');
  assert.equal(vm[3].rows[0].desc, '[VERSION]');
  assert.equal(vm[3].rows[1].desc, 'FOH / Monitor · 48 kHz');
}

// --- default pressed states + option colours ---
{
  const vm = buildSettingsVm(w0.state);
  const spp = vm[0].rows[0];
  assert.deepEqual(spp.options.map(o => o.label), ['8', '12', '16']);
  assert.deepEqual(spp.options.map(o => o.pressed), [false, true, false]);
  assert.deepEqual(spp.options.map(o => o.value), ['8', '12', '16']);
  const sel = spp.options[1];
  assert.equal(sel.bg, '#e8edf5');
  assert.equal(sel.fg, '#0b0f14');
  assert.equal(sel.border, '#e8edf5');
  const unsel = spp.options[0];
  assert.equal(unsel.bg, '#1b222d');
  assert.equal(unsel.fg, '#c9d1dc');
  assert.equal(unsel.border, '#343d4b');
  // Default store: peakHold on, fine off, sof off, muteEnable on, unlocked, confirm mode.
  assert.deepEqual(vm[0].rows[1].options.map(o => o.pressed), [false, true]);
  assert.deepEqual(vm[1].rows[0].options.map(o => o.pressed), [true, false]);
  assert.deepEqual(vm[1].rows[1].options.map(o => o.pressed), [true, false]);
  assert.deepEqual(vm[1].rows[2].options.map(o => o.value)[0], '0');
  assert.deepEqual(vm[1].rows[2].options.map(o => o.pressed)[0], true);
  assert.equal(vm[1].rows[2].options.length, 16);
  assert.equal(vm[1].rows[2].options[15].label, '16');
  assert.deepEqual(vm[2].rows[0].options.map(o => o.pressed), [false, true]);
  assert.deepEqual(vm[2].rows[1].options.map(o => o.pressed), [true, false]);
  assert.deepEqual(vm[2].rows[2].options.map(o => o.label), ['Confirm', 'Instant']);
  assert.deepEqual(vm[2].rows[2].options.map(o => o.pressed), [true, false]);
}

// --- store transitions move the pressed states ---
{
  let w = setSpp(w0, 8);
  assert.deepEqual(buildSettingsVm(w.state)[0].rows[0].options.map(o => o.pressed), [true, false, false]);
  w = setPeakHold(w0, false);
  assert.deepEqual(buildSettingsVm(w.state)[0].rows[1].options.map(o => o.pressed), [true, false]);
  w = setFine(w0, true);
  assert.deepEqual(buildSettingsVm(w.state)[1].rows[0].options.map(o => o.pressed), [false, true]);
  w = setSof(w0, true);
  assert.deepEqual(buildSettingsVm(w.state)[1].rows[1].options.map(o => o.pressed), [false, true]);
  w = setMix(w0, 5);
  const pads = buildSettingsVm(w.state)[1].rows[2].options;
  assert.ok(pads.every((o, k) => o.pressed === (k === 5)));
  w = setMuteEnable(w0, false);
  assert.deepEqual(buildSettingsVm(w.state)[2].rows[0].options.map(o => o.pressed), [true, false]);
  w = setLocked(w0, true);
  assert.deepEqual(buildSettingsVm(w.state)[2].rows[1].options.map(o => o.pressed), [false, true]);
  w = setSceneConfirm(w0, false);
  assert.deepEqual(buildSettingsVm(w.state)[2].rows[2].options.map(o => o.pressed), [false, true]);
}

// --- console description variants (design: offline / connected / null) ---
{
  assert.equal(consoleDescription('foh'), 'FOH / Monitor · 48 kHz');
  assert.equal(consoleDescription('offline'), 'Offline');
  assert.equal(consoleDescription(null), 'Not connected');
  assert.equal(buildSettingsVm({ ...w0.state, connected: 'offline' })[3].rows[1].desc, 'Offline');
  assert.equal(buildSettingsVm({ ...w0.state, connected: null })[3].rows[1].desc, 'Not connected');
}

console.log('settingsVm.test.ts: ok');

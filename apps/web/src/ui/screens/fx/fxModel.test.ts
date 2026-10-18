// F1 FX package: node tests for the pure fx view-model.
// Run: node --experimental-strip-types apps/web/src/ui/screens/fx/fxModel.test.ts
import { strict as assert } from 'node:assert';
import {
  buildFx,
  fxKnobDefs,
  type FxSlot,
} from '../../fixtures/designData.ts';
import {
  FX_ACCENT,
  FX_PARAM_DEFS,
  FX_SLOT_COUNT,
  FX_TYPES,
  buildFxSlotVm,
  buildFxSlots,
  fxIsDelay,
  fxKnobAria,
  fxKnobList,
  fxKnobValueText,
  fxTapLabel,
  fxTitle,
  fxToggleVm,
  fxTypeButtons,
} from './fxModel.ts';

let n = 0;
function ok(cond: unknown, msg: string): void {
  n++;
  assert(cond, msg);
}

// --- slot cards: design fxSlots values --------------------------------------
{
  const fx = buildFx();
  ok(fx.length === FX_SLOT_COUNT, `8 slots, got ${fx.length}`);
  const types = fx.map(s => s.type);
  assert.deepEqual(types, ['Hall', 'Plate', 'Delay', 'Chorus', 'Room', 'Delay', 'Flanger', 'Hall']);
  const slots = buildFxSlots(fx, 0, null);
  ok(slots[0].name === 'FX 1', slots[0].name);
  ok(slots[0].type === 'Hall', slots[0].type);
  ok(slots[0].state === 'On', slots[0].state);
  ok(slots[0].stateColor === '#7be27b', slots[0].stateColor);
  ok(slots[0].ret === 'Ret 1/2', slots[0].ret);
  ok(slots[7].ret === 'Ret 15/16', slots[7].ret);
  ok(slots[7].state === 'Bypassed', slots[7].state);
  ok(slots[7].stateColor === '#f0b44a', slots[7].stateColor);
  ok(slots[0].pressed === true && slots[1].pressed === false, 'selection');
  ok(slots[0].border === '#b28cff' && slots[1].border === '#222a36', 'borders');
  ok(slots[0].bg === '#1b2330' && slots[1].bg === '#0e1520', 'backgrounds');
  ok(slots[0].meterPct === '0.0', 'silent meter with no source');
  // bypassed slot never meters, even with a hot level
  const hot = buildFxSlotVm(fx[7], 7, false, 0);
  ok(hot.meterPct === '0', 'bypassed slot meters 0');
  // on slot with a level follows dbToPos
  const on = buildFxSlotVm({ ...fx[0], on: true }, 0, true, -10);
  ok(on.meterPct === '50.0', `dbToPos(-10)=0.5 -> 50.0, got ${on.meterPct}`);
  // slots beyond the desk's count are removed
  const few = buildFxSlots(fx, 0, null, 6);
  ok(few.length === 6 && few[5].name === 'FX 6', `slotCount=6 -> 6 cards, got ${few.length}`);
}

// --- detail header -----------------------------------------------------------
{
  const fx = buildFx();
  ok(fxTitle(0, fx[0]) === 'FX 1 · Hall', fxTitle(0, fx[0]));
  const t = fxToggleVm(fx[0]);
  ok(t.label === 'On' && t.bg === '#5cdb5c' && t.fg === '#10240f' && t.border === '#5cdb5c', JSON.stringify(t));
  const b = fxToggleVm(fx[7]);
  ok(b.label === 'Bypassed' && b.bg === '#2a1f0a' && b.fg === '#f6c76a' && b.border === '#f0b44a', JSON.stringify(b));
  ok(fxIsDelay(fx[2]) === true && fxIsDelay(fx[0]) === false, 'Delay detection');
  ok(fxTapLabel({ ...fx[2], bpm: null }) === 'Tap tempo', 'tap default');
  ok(fxTapLabel({ ...fx[2], bpm: 120 }) === 'Tap · 120 BPM', 'tap with bpm');
}

// --- type selector ------------------------------------------------------------
{
  assert.deepEqual([...FX_TYPES], ['Hall', 'Plate', 'Room', 'Delay', 'Chorus', 'Flanger']);
  const btns = fxTypeButtons('Hall');
  ok(btns[0].pressed && btns[0].bg === '#b28cff' && btns[0].fg === '#1a0f2e', 'selected type');
  ok(!btns[1].pressed && btns[1].bg === '#2b3038' && btns[1].fg === '#c9d1dc', 'idle type');
  ok(FX_ACCENT === '#b28cff', 'purple accent');
}

// --- descriptor-driven knob lists ---------------------------------------------
{
  for (const t of FX_TYPES) {
    const list = fxKnobList(t, null);
    const specLabels = fxKnobDefs(t).map(d => d.label);
    assert.deepEqual(list.map(k => k.label), specLabels, `${t} labels match fxKnobDefs`);
    ok(list.every(k => typeof k.bind === 'string' && k.bind.startsWith('fx:param:')), `${t} binds`);
  }
  ok(fxKnobList('Hall', null).length === 5, 'reverbs have 5 knobs');
  ok(fxKnobList('Delay', null).length === 5, 'delay has 5 knobs');
  ok(fxKnobList('Chorus', null).length === 4, 'chorus has 4 knobs');
  ok(fxKnobList('Flanger', null).length === 4, 'flanger has 4 knobs');
  // caps filtering removes unsupported params instead of faking them
  const caps = ['fx:param:decay', 'fx:param:mix'];
  const vis = fxKnobList('Hall', caps);
  assert.deepEqual(vis.map(k => k.key), ['decay', 'mix']);
  assert.throws(() => fxKnobList('Nope', null), /unknown fx type/);
}

// --- param formatting per unit -------------------------------------------------
// Hall fixture p = [0.42, 0.15, 0.6, 0.55, 1]
{
  const hall: FxSlot = { type: 'Hall', on: true, p: [0.42, 0.15, 0.6, 0.55, 1], bpm: null };
  assert.deepEqual(
    [0, 1, 2, 3, 4].map(pi => fxKnobValueText(hall, pi)),
    ['4.3 s', '30 ms', '60%', '5.19 kHz', '100%'],
    'Hall fixture texts',
  );
  ok(fxKnobAria(hall, 0) === 'Decay 4.3 s', fxKnobAria(hall, 0));
  // s unit: 0.2 + v*9.8, one decimal
  ok(fxKnobDefs('Hall')[0].fmt(0) === '0.2 s' && fxKnobDefs('Hall')[0].fmt(1) === '10.0 s', 's range');
  // ms unit: integers
  ok(fxKnobDefs('Hall')[1].fmt(0.15) === '30 ms', 'ms');
  // kHz unit via fmtF
  ok(fxKnobDefs('Hall')[3].fmt(0) === '1.00 kHz', fxKnobDefs('Hall')[3].fmt(0));
  ok(fxKnobDefs('Hall')[3].fmt(1) === '20.0 kHz', fxKnobDefs('Hall')[3].fmt(1));
  // Delay: Time fmtMs (20*100^v ms), Feedback %, Hi Cut, Spread, Mix
  const delay: FxSlot = { type: 'Delay', on: true, p: [0.55, 0.35, 0.55, 0.4, 1], bpm: null };
  const dt = [0, 1, 2, 3, 4].map(pi => fxKnobValueText(delay, pi));
  ok(dt[0] === '252 ms', `delay time, got ${dt[0]}`);
  ok(dt[1] === '33%', `delay feedback, got ${dt[1]}`);
  ok(dt[4] === '100%', 'delay mix');
  // Chorus/Flanger: Rate Hz (0.05*100^v), Depth/Spread %, Flanger Feedback 90%
  const chorus: FxSlot = { type: 'Chorus', on: true, p: [0.4, 0.5, 0.6, 1], bpm: null };
  const ct = [0, 1, 2, 3].map(pi => fxKnobValueText(chorus, pi));
  ok(ct[0] === '0.32 Hz', `chorus rate, got ${ct[0]}`);
  ok(ct[1] === '50%' && ct[3] === '100%', 'chorus pct');
  const fl: FxSlot = { type: 'Flanger', on: true, p: [0.3, 0.6, 0.4, 1], bpm: null };
  ok(fxKnobValueText(fl, 2) === '36%', `flanger feedback, got ${fxKnobValueText(fl, 2)}`);
  // every type's defs accept the slot's stored p length
  for (const t of FX_TYPES) {
    const defs = fxKnobDefs(t);
    ok(defs.length === FX_PARAM_DEFS[t].length, `${t} def count`);
    defs.forEach((d, i) => ok(typeof d.fmt(0.5) === 'string' && d.fmt(0.5).length > 0, `${t}[${i}] formats`));
  }
  assert.throws(() => fxKnobValueText(hall, 5), /out of range/);
}

console.log(`fxModel.test.ts: ok (${n} assertions)`);

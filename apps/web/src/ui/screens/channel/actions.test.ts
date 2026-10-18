// W5b channel action tests: ui.setInputKnob / channel.eqNode /
// channel.dynKnob wrap the store (lock + spec rules apply).
// Run: node --experimental-strip-types apps/web/src/ui/screens/channel/actions.test.ts
import { strict as assert } from 'node:assert';
import {
  clearActions,
  clearActionWarnings,
  dispatchAction,
  getAction,
  registerBuiltins,
  type ActionCtx,
} from '../../modules/actions.ts';
import { registerChannelActions } from './actions.ts';
import {
  initialWorld,
  setDynParam,
  setEqParam,
  setSpecValue,
  type World,
} from '../../state/uiLogic.ts';
import { spec, fromN, toN } from '../../fixtures/designData.ts';

clearActions();
registerBuiltins();
registerChannelActions();
clearActionWarnings();

function warns(): { list: string[]; fn: (m: string) => void } {
  const list: string[] = [];
  return { list, fn: (m: string) => list.push(m) };
}

// Fake ui.do backed by the real pure uiLogic transitions (mirrors uiStore).
function fakeStore(world: { w: World }): Record<string, (...a: never[]) => unknown> {
  return {
    setInputKnob: (...a: never[]) => {
      const [id, key, v] = a as [string, 'gain' | 'trim' | 'delay', number];
      world.w = setSpecValue(world.w, id, key, (c, nv) => {
        if (!c.input) return;
        if (key === 'gain') c.input.gain = nv;
        else if (key === 'trim') c.input.trim = nv;
        else c.input.delay = nv;
      }, toN(spec(key), v));
    },
    setEqParam: (...a: never[]) => {
      world.w = setEqParam(world.w, a[0] as string, a[1] as number, a[2] as 'f' | 'g' | 'q', a[3] as number);
    },
    setDynParam: (...a: never[]) => {
      world.w = setDynParam(world.w, a[0] as string, a[1] as 'gate' | 'comp', a[2] as string, a[3] as number);
    },
  };
}

function ctxFor(world: { w: World }, w: { list: string[]; fn: (m: string) => void }): ActionCtx {
  return { ui: fakeStore(world), warn: w.fn };
}

// --- registration ---
{
  assert.ok(getAction('ui.setInputKnob'), 'ui.setInputKnob registered');
  assert.ok(getAction('channel.eqNode'), 'channel.eqNode registered');
  assert.ok(getAction('channel.dynKnob'), 'channel.dynKnob registered');
}

// --- ui.setInputKnob runs through the store spec mapping ---
{
  const world = { w: initialWorld() };
  const w = warns();
  const before = world.w.chans['in1']?.input?.gain;
  const r = dispatchAction(
    { do: 'ui.setInputKnob', args: { id: 'in1', key: 'gain', value: 40 } },
    {},
    ctxFor(world, w),
  );
  assert.ok(r.ok, 'dispatch ok');
  assert.notEqual(world.w.chans['in1']?.input?.gain, before, 'gain changed');
  assert.deepEqual(w.list, [], 'no warnings');
}

// --- channel.eqNode: band maps to freq + gain, HPF to freq only ---
{
  const world = { w: initialWorld() };
  const w = warns();
  let r = dispatchAction(
    { do: 'channel.eqNode', args: { id: 'in1', band: 2, f: 400, g: 3, locked: false } },
    {},
    ctxFor(world, w),
  );
  assert.ok(r.ok);
  assert.equal(world.w.chans['in1']?.eq?.bands[1]?.f, 400, 'band freq set');
  assert.equal(world.w.chans['in1']?.eq?.bands[1]?.g, 3, 'band gain set');

  r = dispatchAction(
    { do: 'channel.eqNode', args: { id: 'in1', band: 0, f: 120, g: 9, locked: false } },
    {},
    ctxFor(world, w),
  );
  assert.ok(r.ok);
  assert.equal(world.w.chans['in1']?.eq?.hpf.f, 120, 'hpf freq set');
}

// --- channel.eqNode: locked drops the edit (selection is separate) ---
{
  const world = { w: initialWorld() };
  const w = warns();
  const before = world.w.chans['in1']?.eq?.bands[1]?.f;
  const r = dispatchAction(
    { do: 'channel.eqNode', args: { id: 'in1', band: 2, f: 999, g: 9, locked: true } },
    {},
    ctxFor(world, w),
  );
  assert.ok(r.ok);
  assert.equal(world.w.chans['in1']?.eq?.bands[1]?.f, before, 'locked: no edit');
}

// --- channel.dynKnob: raw value mapped through dynSpecKey + toN ---
{
  const world = { w: initialWorld() };
  const w = warns();
  const raw = -10;
  const r = dispatchAction(
    { do: 'channel.dynKnob', args: { id: 'in1', which: 'comp', prop: 'thr', value: raw, locked: false } },
    {},
    ctxFor(world, w),
  );
  assert.ok(r.ok);
  const want = toN(spec('cthr'), raw);
  const expected = fromN(spec('cthr'), want);
  assert.equal(world.w.chans['in1']?.dyn?.comp.thr, expected, 'thr mapped through spec');
}

// --- channel.dynKnob: locked drops the edit; unknown prop warns ---
{
  const world = { w: initialWorld() };
  const w = warns();
  const before = world.w.chans['in1']?.dyn?.comp.thr;
  let r = dispatchAction(
    { do: 'channel.dynKnob', args: { id: 'in1', which: 'comp', prop: 'thr', value: 0, locked: true } },
    {},
    ctxFor(world, w),
  );
  assert.ok(r.ok);
  assert.equal(world.w.chans['in1']?.dyn?.comp.thr, before, 'locked: no edit');

  r = dispatchAction(
    { do: 'channel.dynKnob', args: { id: 'in1', which: 'comp', prop: 'nope', value: 0, locked: false } },
    {},
    ctxFor(world, w),
  );
  assert.ok(r.ok, 'unknown prop warns, never throws');
  assert.ok(w.list.some(m => m.includes('unknown dyn param')), 'warned: ' + w.list.join('; '));
}

// --- arg type errors fail the dispatch, never throw ---
{
  const world = { w: initialWorld() };
  const w = warns();
  const r = dispatchAction(
    { do: 'channel.eqNode', args: { id: 'in1', band: 'two', f: 1, g: 1, locked: false } },
    {},
    ctxFor(world, w),
  );
  assert.ok(!r.ok, 'bad band type fails');
}

// --- missing store fn warns and skips (never throws) ---
{
  const w = warns();
  const r = dispatchAction(
    { do: 'ui.setInputKnob', args: { id: 'in1', key: 'gain', value: 1 } },
    {},
    { ui: {}, warn: w.fn },
  );
  assert.ok(r.ok);
  assert.ok(w.list.some(m => m.includes('setInputKnob')), 'warned: ' + w.list.join('; '));
}

console.log('channel actions.test.ts: ok');

// L1 tests: action registry, dispatch + sequences + $expr args, validators,
// lock/muteEnable enforcement through the store, profile ops.
// Run: node --experimental-strip-types apps/web/src/ui/modules/actions.test.ts
import { strict as assert } from 'node:assert';
import {
  bindUiActions,
  clearActions,
  clearActionWarnings,
  dispatchAction,
  getAction,
  listActions,
  registerAction,
  registerBuiltins,
  resolveArgs,
  type ActionCtx,
} from './actions.ts';
import {
  initialWorld,
  patchSrc,
  setFader,
  toggleMute,
  type World,
} from '../state/uiLogic.ts';

function warns(): { list: string[]; fn: (m: string) => void } {
  const list: string[] = [];
  return { list, fn: (m: string) => list.push(m) };
}

// Fake ui.do backed by the real pure uiLogic transitions: proves handlers run
// through the store (lock/muteEnable rules apply).
function fakeStore(world: { w: World }): Record<string, (...a: never[]) => unknown> {
  return {
    setPatchSel: (...a: never[]) => { world.w = { ...world.w, state: { ...world.w.state, patchSel: a[0] as string } }; },
    patchSrc: (...a: never[]) => { world.w = patchSrc(world.w, a[0] as string); },
    unpatch: () => { world.w = { ...world.w, chans: { ...world.w.chans } }; },
    setFader: (...a: never[]) => { world.w = setFader(world.w, a[0] as string, a[1] as number); },
    toggleMute: (...a: never[]) => { world.w = toggleMute(world.w, a[0] as string); },
    setLocked: (...a: never[]) => { world.w = { ...world.w, state: { ...world.w.state, locked: a[0] as boolean } }; },
  };
}

clearActions();
registerBuiltins();
clearActionWarnings();

// --- registry ---
{
  assert.ok(getAction('ui.setPatchSel'), 'builtin present');
  assert.ok(getAction('ui.scan'), 'scan present');
  assert.ok(getAction('nav.back'), 'nav present');
  assert.ok(getAction('profile.undo'), 'profile present');
  assert.ok(getAction('engine.setParam'), 'engine placeholder present');
  assert.equal(getAction('nope'), undefined);
  const ids = listActions().map(a => a.id);
  assert.ok(ids.includes('ui.toggleMute') && ids.includes('ui.patchSrc') && ids.includes('ui.connect'));
  assert.ok(registerAction('demo.tap', () => {}, { describe: 'demo' }));
  assert.ok(getAction('demo.tap'));
  assert.equal(registerAction('', () => false), false);
}

// --- dispatch basics: literals, sequences, unknown ids warn + skip ---
{
  const world = { w: initialWorld() };
  const calls: unknown[][] = [];
  registerAction('test.rec', (a) => { calls.push([a['x']]); });
  const w = warns();
  const r = dispatchAction(
    [{ do: 'test.rec', args: { x: 1 } }, { do: 'test.rec', args: { x: 2 } }, { do: 'missing.action' }],
    {},
    { ui: fakeStore(world), warn: w.fn },
  );
  assert.equal(r.ok, true);
  assert.deepEqual(calls, [[1], [2]]);
  assert.ok(w.list.some(m => m.includes('missing.action')), w.list.join('; '));
  // unknown action on its own still ok (node kept, event skipped)
  const w2 = warns();
  assert.equal(dispatchAction({ do: 'missing.action' }, {}, { ui: {}, warn: w2.fn }).ok, true);
}

// --- $expr arg refs evaluate in scope (payload/item/index) ---
{
  const seen: Record<string, unknown>[] = [];
  registerAction('test.args', (a) => { seen.push(a); });
  const r = resolveArgs(
    {
      id: { $expr: 'item.id' },
      n: { $expr: 'index + 1' },
      key: { $expr: 'payload.key' },
      lit: 'Lcl 01',
      num: 3,
      nested: { label: { $expr: 'upper(item.id)' }, tags: [{ $expr: 'item.id' }, 'x'] },
    },
    { item: { id: 'in2' }, index: 4, payload: { key: 'foh' } },
    () => {},
  );
  assert.deepEqual(r, {
    id: 'in2', n: 5, key: 'foh', lit: 'Lcl 01', num: 3,
    nested: { label: 'IN2', tags: ['in2', 'x'] },
  });
  const w = warns();
  const r2 = dispatchAction({ do: 'test.args', args: { bad: { $expr: 'nope(' } } }, {}, { ui: {}, warn: w.fn });
  assert.equal(r2.ok, true); // bad $expr warns, resolves undefined, handler still runs
  assert.ok(w.list.some(m => m.includes('$expr')), w.list.join('; '));
  assert.equal(seen[seen.length - 1]['bad'], undefined);
  // unknown $ directive is a validation error
  const { validateNode } = await import('./validate.ts');
  const v = validateNode({ id: 'x', module: 'btn', on: { press: { do: 'test.args', args: { q: { $nope: 1 } } } } });
  assert.equal(v.ok, false, JSON.stringify(v.errors));
}

// --- arg schema checks ---
{
  const w = warns();
  const r = dispatchAction({ do: 'ui.setFader', args: { id: 'in1', value: 'loud' } }, {}, { ui: {}, warn: w.fn });
  assert.equal(r.ok, false);
  assert.ok(w.list.some(m => m.includes('must be number')));
}

// --- lock / muteEnable enforced via the store ---
{
  const world = { w: initialWorld() };
  const ui = fakeStore(world);
  const w = warns();
  // patch advances selection (uiLogic rule through the store)
  dispatchAction({ do: 'ui.patchSrc', args: { label: 'Lcl 05' } }, {}, { ui, warn: w.fn });
  assert.equal(world.w.chans['in1'].src, 'Lcl 05');
  assert.equal(world.w.state.patchSel, 'in2');
  // locked: patch is a no-op in the store
  const beforeSrc = world.w.chans['in2'].src;
  dispatchAction({ do: 'ui.setLocked', args: { value: true } }, {}, { ui, warn: w.fn });
  dispatchAction({ do: 'ui.patchSrc', args: { label: 'Lcl 06' } }, {}, { ui, warn: w.fn });
  assert.equal(world.w.chans['in2'].src, beforeSrc); // unchanged
  assert.equal(world.w.state.patchSel, 'in2'); // no advance
  // mute honours muteEnable + lock: disable both gates, mute flips
  const world2 = { w: initialWorld() };
  const ui2 = fakeStore(world2);
  world2.w = { ...world2.w, state: { ...world2.w.state, muteEnable: false } };
  dispatchAction({ do: 'ui.toggleMute', args: { id: 'in1' } }, {}, { ui: ui2, warn: w.fn });
  assert.equal(world2.w.chans['in1'].mute, false);
  world2.w = { ...world2.w, state: { ...world2.w.state, muteEnable: true } };
  dispatchAction({ do: 'ui.toggleMute', args: { id: 'in1' } }, {}, { ui: ui2, warn: w.fn });
  assert.equal(world2.w.chans['in1'].mute, true);
  // setFader lock-checked through the store
  dispatchAction({ do: 'ui.setFader', args: { id: 'in1', value: 0.9 } }, {}, { ui: ui2, warn: w.fn });
  assert.equal(world2.w.chans['in1'].fader > 0.8, true);
}

// --- throwing handlers warn and stop the sequence ---
{
  registerAction('test.boom', () => { throw new Error('kaput'); });
  const calls: string[] = [];
  registerAction('test.after', () => { calls.push('after'); });
  const w = warns();
  const r = dispatchAction([{ do: 'test.boom' }, { do: 'test.after' }], {}, { ui: {}, warn: w.fn });
  assert.equal(r.ok, false);
  assert.deepEqual(calls, []);
  assert.ok(w.list.some(m => m.includes('kaput')));
}

// --- profile.* actions run against the bound profile store ---
{
  const ops: unknown[] = [];
  let undone = 0, redone = 0, reset = 0;
  const profile = {
    commit: (op: unknown) => { ops.push(op); return true; },
    undo: () => { undone++; return true; },
    redo: () => { redone++; return true; },
    reset: () => { reset++; },
  };
  const w = warns();
  bindUiActions({}, { profile });
  dispatchAction({ do: 'profile.applyOp', args: { op: { op: 'hide', id: 'a', hidden: true } } }, {}, { warn: w.fn });
  dispatchAction({ do: 'profile.undo' }, {}, { warn: w.fn });
  dispatchAction({ do: 'profile.redo' }, {}, { warn: w.fn });
  dispatchAction({ do: 'profile.reset' }, {}, { warn: w.fn });
  assert.deepEqual(ops, [{ op: 'hide', id: 'a', hidden: true }]);
  assert.equal(undone, 1);
  assert.equal(redone, 1);
  assert.equal(reset, 1);
  bindUiActions({});
}

// --- engine.* placeholders warn + no-op, never throw ---
{
  const w = warns();
  const r = dispatchAction({ do: 'engine.setParam', args: { id: 'fader', value: 1 } }, {}, { ui: {}, warn: w.fn });
  assert.equal(r.ok, true);
  assert.ok(w.list.some(m => m.includes('wave-3')), w.list.join('; '));
}

// --- cross-check: every ui.do action the routing/connection screens call exists ---
{
  for (const id of ['ui.setRouteBank', 'ui.setPatchSel', 'ui.setPatchGroup', 'ui.patchSrc', 'ui.unpatch', 'ui.scan', 'ui.disconnect', 'ui.connect', 'ui.goOffline']) {
    assert.ok(getAction(id), id);
  }
}

console.log('actions.test.ts: ok');
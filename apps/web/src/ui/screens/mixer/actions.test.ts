// W5c Mixer actions test: ui.setStripFader dispatches to setSend (SOF) or
// setFader through the bound store, and warns (never throws) on bad input.
// Run: node --experimental-strip-types apps/web/src/ui/screens/mixer/actions.test.ts
import { strict as assert } from 'node:assert';
import { clearActions, dispatchAction, getAction, registerBuiltins } from '../../modules/actions.ts';
import './actions.ts';

function ctx(calls: unknown[][]) {
  const warns: string[] = [];
  return {
    ui: {
      setFader: (...a: unknown[]): void => { calls.push(['setFader', ...a]); },
      setSend: (...a: unknown[]): void => { calls.push(['setSend', ...a]); },
    },
    warn: (m: string): void => { warns.push(m); },
    _warns: warns,
  };
}

{
  const calls: unknown[][] = [];
  const c = ctx(calls);
  const r = dispatchAction(
    { do: 'ui.setStripFader', args: { id: 'in1', value: 0.5, useSend: false, mix: 0 } },
    {},
    c,
  );
  assert.ok(r.ok);
  assert.deepEqual(calls, [['setFader', 'in1', 0.5]]);
  assert.deepEqual(c._warns, []);
}

{
  const calls: unknown[][] = [];
  const c = ctx(calls);
  const r = dispatchAction(
    { do: 'ui.setStripFader', args: { id: 'in1', value: 0.25, useSend: true, mix: 3 } },
    {},
    c,
  );
  assert.ok(r.ok);
  assert.deepEqual(calls, [['setSend', 'in1', 3, 0.25]]);
}

{
  const calls: unknown[][] = [];
  const c = ctx(calls);
  const r = dispatchAction(
    { do: 'ui.setStripFader', args: { id: 7, value: 'x', useSend: false, mix: 0 } },
    {},
    c,
  );
  assert.ok(!r.ok, 'arg type mismatch warns and stops (never throws)');
  assert.deepEqual(calls, []);
  assert.ok(c._warns.length >= 1);
}

{
  // builtins the mixer layout uses are registered (validateLayout sees them).
  clearActions();
  registerBuiltins();
  for (const id of ['ui.setFader', 'ui.setSend', 'ui.setPan', 'ui.toggleMute', 'ui.select', 'ui.openEq', 'ui.openDyn']) {
    assert.ok(getAction(id), `builtin missing: ${id}`);
  }
}

console.log('mixer actions.test.ts: ok');

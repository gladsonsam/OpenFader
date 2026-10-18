// W5a Settings actions test: ui.setSetting maps descriptor keys to store calls.
// Run: node --experimental-strip-types apps/web/src/ui/screens/settings/actions.test.ts
import { strict as assert } from 'node:assert';
import {
  clearActions,
  dispatchAction,
  getAction,
  registerBuiltins,
} from '../../modules/actions.ts';
import { registerSettingsActions } from './actions.ts';

clearActions();
registerBuiltins();
registerSettingsActions();

function fake(): { calls: Array<{ name: string; args: unknown[] }>; ui: Record<string, (...a: never[]) => unknown> } {
  const calls: Array<{ name: string; args: unknown[] }> = [];
  const names = ['setSpp', 'setPeakHold', 'setFine', 'setSof', 'setMix', 'setMuteEnable', 'setLocked', 'setSceneConfirm'];
  const ui: Record<string, (...a: never[]) => unknown> = {};
  for (const n of names) ui[n] = (...a: never[]) => { calls.push({ name: n, args: a as unknown[] }); };
  return { calls, ui };
}

function warns(): { list: string[]; fn: (m: string) => void } {
  const list: string[] = [];
  return { list, fn: (m: string) => list.push(m) };
}

{
  assert.ok(getAction('ui.setSetting'), 'ui.setSetting registered');
  const meta = getAction('ui.setSetting');
  assert.deepEqual(meta?.meta.args, { key: 'string', value: 'any' });
}

function run(key: string, value: string): { calls: Array<{ name: string; args: unknown[] }>; list: string[] } {
  const f = fake();
  const w = warns();
  const r = dispatchAction(
    { do: 'ui.setSetting', args: { key, value } },
    {},
    { ui: f.ui, warn: w.fn },
  );
  assert.equal(r.ok, true, `${key}=${value} dispatches`);
  return { calls: f.calls, list: w.list };
}

// Every descriptor key with a control decodes like the old onPick did.
{
  assert.deepEqual(run('spp', '8').calls, [{ name: 'setSpp', args: [8] }]);
  assert.deepEqual(run('spp', '16').calls, [{ name: 'setSpp', args: [16] }]);
  assert.deepEqual(run('peakHold', 'on').calls, [{ name: 'setPeakHold', args: [true] }]);
  assert.deepEqual(run('peakHold', 'off').calls, [{ name: 'setPeakHold', args: [false] }]);
  assert.deepEqual(run('fine', 'on').calls, [{ name: 'setFine', args: [true] }]);
  assert.deepEqual(run('sof', 'off').calls, [{ name: 'setSof', args: [false] }]);
  assert.deepEqual(run('mix', '5').calls, [{ name: 'setMix', args: [5] }]);
  assert.deepEqual(run('muteEnable', 'off').calls, [{ name: 'setMuteEnable', args: [false] }]);
  assert.deepEqual(run('locked', 'on').calls, [{ name: 'setLocked', args: [true] }]);
  assert.deepEqual(run('sceneConfirm', 'confirm').calls, [{ name: 'setSceneConfirm', args: [true] }]);
  assert.deepEqual(run('sceneConfirm', 'instant').calls, [{ name: 'setSceneConfirm', args: [false] }]);
}

// Unknown keys warn and never throw (a profile-added setting with no store
// mapping stays inert instead of breaking the row).
{
  const { calls, list } = run('appVersion', 'x');
  assert.deepEqual(calls, []);
  assert.ok(list.some(m => m.includes('appVersion')), list.join('; '));
  const w = warns();
  const r = dispatchAction({ do: 'ui.setSetting', args: { key: 'nope' } }, {}, { ui: {}, warn: w.fn });
  assert.equal(r.ok, true);
}

console.log('settings actions.test.ts: ok');

// L0 tests: registry rules (validate / duplicate / freeze / palette).
import { strict as assert } from 'node:assert';
import { clearRegistry, getModule, listModules, queryPalette, registerModule, validateManifest } from './registry.ts';

const comp = { __fakeComponent: true };

clearRegistry();

// --- valid manifest registers ---
{
  const r = registerModule({
    id: 'text',
    title: 'Text',
    category: 'display',
    component: comp as never,
    props: { value: { type: 'text', default: '' } },
    defaults: { value: '' },
    size: {},
  });
  assert.deepEqual(r, { ok: true });
  assert.equal(getModule('text')?.title, 'Text');
}

// --- invalid manifests rejected, never throw ---
{
  const bad: unknown[] = [
    null,
    {},
    { id: 'x' },
    { id: 'Text!', title: 'T', category: 'display', component: comp, props: {}, defaults: {}, size: {} },
    { id: 'x', title: '', category: 'display', component: comp, props: {}, defaults: {}, size: {} },
    { id: 'x', title: 'T', category: 'nope', component: comp, props: {}, defaults: {}, size: {} },
    { id: 'x', title: 'T', category: 'display', props: {}, defaults: {}, size: {} },
    { id: 'x', title: 'T', category: 'display', component: comp, props: [], defaults: {}, size: {} },
    { id: 'x', title: 'T', category: 'display', component: comp, props: {}, size: {} },
    { id: 'x', title: 'T', category: 'display', component: comp, props: {}, defaults: {}, size: {}, fallback: 'explode' },
    { id: 'x', title: 'T', category: 'display', component: comp, props: {}, defaults: {}, size: {}, roles: ['admin'] },
    { id: 'x', title: 'T', category: 'display', component: comp, props: {}, defaults: {}, size: {}, binds: 'param:fader' },
  ];
  for (const m of bad) {
    assert.ok(validateManifest(m).length > 0, `should reject ${JSON.stringify(m)}`);
    assert.equal(registerModule(m as never).ok, false);
  }
}

// --- duplicates rejected; stored manifest frozen ---
{
  const r = registerModule({
    id: 'text',
    title: 'Other',
    category: 'display',
    component: comp as never,
    props: {},
    defaults: {},
    size: {},
  });
  assert.equal(r.ok, false);
  assert.match((r as { errors: string[] }).errors.join(), /duplicate/);
  assert.equal(getModule('text')?.title, 'Text');
  assert.equal(Object.isFrozen(getModule('text')), true);
}

// --- list + palette ---
{
  registerModule({
    id: 'fader-x',
    title: 'Channel Fader',
    category: 'strip',
    component: comp as never,
    props: {},
    defaults: {},
    size: { flex: true },
  });
  assert.deepEqual(listModules('strip').map(m => m.id), ['fader-x']);
  assert.equal(listModules().length, 2);
  assert.deepEqual(queryPalette('fad').map(m => m.id), ['fader-x']);
  assert.deepEqual(queryPalette('TEXT').map(m => m.id), ['text']);
  assert.deepEqual(queryPalette('', 'strip').map(m => m.id), ['fader-x']);
  assert.deepEqual(queryPalette('zzz'), []);
  assert.equal(getModule('nope'), undefined);
}

clearRegistry();
console.log('registry.test.ts: ok');

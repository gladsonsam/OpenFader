// L0 tests: profile ops (set/move/insert/remove/hide), defaults-update
// survival, reset, migrate, import/export garbage rejection, undo/redo
// bounds, locked profiles.
import { strict as assert } from 'node:assert';
import { applyOp, applyProfile, diffFromDefault, resetAll, resetNode } from './profile/ops.ts';
import { createHistory, pushHistory, redoHistory, undoHistory } from './profile/history.ts';
import { emptyProfile, exportProfile, importProfile } from './profile/io.ts';
import { migrateProfile } from './profile/migrate.ts';
import type { LayoutNode } from './schema.ts';

const defaults = (): LayoutNode => ({
  id: 'root',
  module: 'col',
  props: { gap: 8 },
  children: [
    { id: 'a', module: 'text', props: { value: 'A' } },
    { id: 'b', module: 'text', props: { value: 'B' } },
    {
      id: 'box',
      module: 'row',
      props: { gap: 2 },
      children: [{ id: 'c', module: 'text', props: { value: 'C' } }],
    },
  ],
});

// --- set / hide ---
{
  const t = defaults();
  assert.deepEqual(applyOp(t, { op: 'set', id: 'a', path: 'props.value', value: 'A2' }), { ok: true });
  assert.equal(t.children?.[0].props?.['value'], 'A2');
  assert.deepEqual(applyOp(t, { op: 'hide', id: 'b', hidden: true }), { ok: true });
  assert.equal(t.children?.[1].hidden, true);
  // bad paths / unknown ids / root removal rejected, never throw
  assert.equal(applyOp(t, { op: 'set', id: 'zzz', path: 'props.value', value: 1 }).ok, false);
  assert.equal(applyOp(t, { op: 'remove', id: 'root' }).ok, false);
  assert.equal(applyOp(t, { op: 'move', id: 'root', parent: 'box', index: 0 }).ok, false);
  assert.equal(applyOp(t, { op: 'whatever' } as never).ok, false);
}

// --- remove / insert / move ---
{
  const t = defaults();
  assert.deepEqual(applyOp(t, { op: 'remove', id: 'b' }), { ok: true });
  assert.deepEqual(t.children?.map(c => c.id), ['a', 'box']);
  assert.deepEqual(
    applyOp(t, { op: 'insert', parent: 'box', index: 5, node: { id: 'd', module: 'text' } }),
    { ok: true },
  );
  assert.deepEqual(t.children?.[1].children?.map(c => c.id), ['c', 'd']);
  assert.deepEqual(applyOp(t, { op: 'move', id: 'd', parent: 'root', index: 0 }), { ok: true });
  assert.deepEqual(t.children?.map(c => c.id), ['d', 'a', 'box']);
  // move into own descendant rejected; duplicate insert rejected
  assert.equal(applyOp(t, { op: 'move', id: 'box', parent: 'c', index: 0 }).ok, false);
  assert.equal(applyOp(t, { op: 'insert', parent: 'root', index: 0, node: { id: 'a', module: 'text' } }).ok, false);
}

// --- applyProfile: ops survive a defaults update that adds node Y ---
{
  const profile = { ...emptyProfile('p'), ops: [{ op: 'set', id: 'a', path: 'props.value', value: 'A2' }] as never };
  const updated: LayoutNode = {
    ...defaults(),
    children: [...(defaults().children ?? []), { id: 'y', module: 'text', props: { value: 'Y' } }],
  };
  const { tree, skipped } = applyProfile(updated, profile);
  assert.equal(skipped.length, 0);
  assert.equal(tree.children?.[0].props?.['value'], 'A2');
  assert.equal(tree.children?.[3].id, 'y');
  // Stale op referencing a deleted node is skipped, never fatal.
  const stale = { ...emptyProfile('p2'), ops: [{ op: 'set', id: 'gone', path: 'props.value', value: 1 }] as never };
  const r2 = applyProfile(defaults(), stale);
  assert.equal(r2.skipped.length, 1);
  assert.equal(r2.tree.children?.length, 3);
}

// --- diffFromDefault round-trips ---
{
  const cur = defaults();
  cur.children![0].props = { value: 'A2' };
  cur.children![1].hidden = true;
  const [moved] = cur.children!.splice(2, 1);
  cur.children!.unshift(moved);
  const ops = diffFromDefault(defaults(), cur);
  assert.ok(ops.some(o => o.op === 'set' && o.id === 'a'), JSON.stringify(ops));
  assert.ok(ops.some(o => o.op === 'hide' && o.id === 'b'), JSON.stringify(ops));
  const { tree } = applyProfile(defaults(), { ops, locked: false });
  assert.equal(tree.children?.[0].id, 'box');
  assert.equal(tree.children?.[1].props?.['value'], 'A2');
  assert.equal(tree.children?.[2].hidden, true);
}

// --- reset ---
{
  const t = defaults();
  t.children![0].props = { value: 'changed' };
  const r = resetNode(t, defaults(), 'a');
  assert.equal(r.ok, true);
  assert.equal(r.tree.children?.[0].props?.['value'], 'A');
  assert.equal(t.children?.[0].props?.['value'], 'changed'); // pure: input untouched
  assert.equal(resetNode(t, defaults(), 'zzz').ok, false);
  assert.deepEqual(resetAll(defaults()), defaults());
}

// --- locked profiles reject mutations ---
{
  const t = defaults();
  assert.equal(applyOp(t, { op: 'set', id: 'a', path: 'props.value', value: 'x' }, true).ok, false);
  assert.equal(t.children?.[0].props?.['value'], 'A');
  const locked = { ...emptyProfile('l'), locked: true, ops: [{ op: 'set', id: 'a', path: 'props.value', value: 'x' }] as never };
  const r = applyProfile(defaults(), locked);
  assert.equal(r.tree.children?.[0].props?.['value'], 'A');
  assert.equal(r.skipped.length, 1);
}

// --- migrate v1 -> v3 (via v2): ops become set/hide, banks become keys ---
{
  const v1 = {
    v: 1,
    id: 'old',
    ops: [{ op: 'setProp', id: 'a', key: 'gap', value: 4 }],
    hidden: ['b'],
    theme: {},
    banks: [],
    settings: {},
    pages: [],
    locked: false,
  };
  const m = migrateProfile(v1);
  assert.equal(m.ok, true);
  const doc = (m as { profile: Record<string, unknown> }).profile;
  assert.equal(doc['v'], 3);
  assert.ok(!(doc['hidden'] !== undefined && Array.isArray(doc['ops']) === false));
  const ops = doc['ops'] as Array<Record<string, unknown>>;
  assert.ok(ops.some(o => o['op'] === 'set' && o['path'] === 'props.gap'));
  assert.ok(ops.some(o => o['op'] === 'hide' && o['id'] === 'b'));
  // v2 banks with raw indices become stable strip keys through the sim map.
  const v2 = {
    v: 2,
    id: 'banks',
    ops: [],
    theme: {},
    banks: [{ id: 'main', name: 'Main', items: [0, '1', 32, 72, 'mix/01', 'ch/1', 'main/1', 'nope'] }],
    settings: {},
    pages: [],
    locked: false,
  };
  const m2 = migrateProfile(v2);
  assert.equal(m2.ok, true);
  const doc2 = (m2 as { profile: Record<string, unknown> }).profile;
  assert.equal(doc2['v'], 3);
  const items = (doc2['banks'] as Array<Record<string, unknown>>)[0]!['items'];
  assert.deepEqual(items, ['ch/01', 'ch/02', 'bus/01', 'main', 'bus/01', 'ch/01', 'main', 'nope']);
  // newer-than-supported rejected; garbage rejected, never throws
  assert.equal(migrateProfile({ v: 99 }).ok, false);
  assert.equal(migrateProfile(null).ok, false);
  assert.equal(migrateProfile('x').ok, false);
}

// --- import/export: garbage rejected, v1 migrated, round-trip stable ---
{
  for (const bad of ['', '{', '[]', 'null', '{}', '{"v":2}', '{"v":99,"id":"x"}']) {
    assert.equal(importProfile(bad).ok, false, bad);
  }
  const p = { ...emptyProfile('rt'), ops: [{ op: 'hide', id: 'a', hidden: true }] as never };
  const json = exportProfile(p);
  const back = importProfile(json);
  assert.equal(back.ok, true);
  assert.deepEqual((back as unknown as { profile: typeof p }).profile.ops, p.ops);
  // v1 JSON imports through migration
  const v1json = JSON.stringify({ v: 1, id: 'o', ops: [], theme: {}, banks: [], settings: {}, pages: [], locked: false });
  assert.equal(importProfile(v1json).ok, true);
}

// --- undo/redo bounds (pure, bounded) ---
{
  let h = createHistory<string[]>(['a']);
  h = pushHistory(h, ['a', 'b']);
  h = pushHistory(h, ['a', 'b', 'c']);
  assert.deepEqual(undoHistory(h).present, ['a', 'b']);
  assert.deepEqual(redoHistory(undoHistory(h)).present, ['a', 'b', 'c']);
  assert.deepEqual(undoHistory(createHistory(['x'])).present, ['x']); // underflow no-op
  assert.deepEqual(redoHistory(createHistory(['x'])).present, ['x']); // overflow no-op
  let bounded = createHistory<number[]>([], 3);
  for (let i = 0; i < 10; i++) bounded = pushHistory(bounded, [i]);
  assert.ok(bounded.past.length <= 3, String(bounded.past.length));
  // push clears redo
  const h2 = redoHistory(pushHistory(undoHistory(h), ['z']));
  assert.deepEqual(h2.future, []);
}

// --- L1: profile ops can set on.click and bind.* paths; round-trip ---
{
  const t = defaults();
  assert.deepEqual(applyOp(t, { op: 'set', id: 'a', path: 'on.click', value: { do: 'ui.goBack' } }), { ok: true });
  assert.deepEqual((t.children?.[0] as unknown as Record<string, unknown>)['on'], { click: { do: 'ui.goBack' } });
  assert.deepEqual(applyOp(t, { op: 'set', id: 'a', path: 'bind.title', value: 'store.mode' }), { ok: true });
  assert.deepEqual((t.children?.[0] as unknown as Record<string, unknown>)['bind'], { title: 'store.mode' });
  // insert op carrying on + bind (the "user adds a working button" flow)
  assert.deepEqual(
    applyOp(t, {
      op: 'insert',
      parent: 'root',
      index: 0,
      node: { id: 'btn', module: 'btn', props: { label: 'Go' }, bind: { label: "'x'" }, on: { press: { do: 'ui.goBack' } } },
    }),
    { ok: true },
  );
  const ins = t.children?.[0] as unknown as Record<string, unknown>;
  assert.equal(ins['id'], 'btn');
  assert.deepEqual(ins['on'], { press: { do: 'ui.goBack' } });
  // export/import round-trips on + bind ops byte-identical
  const p = { ...emptyProfile('rt2'), ops: [
    { op: 'set', id: 'a', path: 'on.click', value: { do: 'ui.goBack' } },
    { op: 'set', id: 'a', path: 'bind.title', value: 'store.mode' },
  ] as never };
  const back = importProfile(exportProfile(p));
  assert.equal(back.ok, true);
  assert.deepEqual((back as unknown as { profile: typeof p }).profile.ops, p.ops);
  // diffFromDefault picks up bind/on/when edits and replays them
  const cur = defaults();
  (cur.children![0] as unknown as Record<string, unknown>)['on'] = { click: { do: 'ui.goBack' } };
  (cur.children![0] as unknown as Record<string, unknown>)['bind'] = { title: 'store.mode' };
  (cur.children![1] as unknown as Record<string, unknown>)['when'] = 'store.locked';
  const ops = diffFromDefault(defaults(), cur);
  assert.ok(ops.some(o => o.op === 'set' && o.id === 'a' && o.path === 'on'), JSON.stringify(ops));
  assert.ok(ops.some(o => o.op === 'set' && o.id === 'a' && o.path === 'bind'), JSON.stringify(ops));
  assert.ok(ops.some(o => o.op === 'set' && o.id === 'b' && o.path === 'when'), JSON.stringify(ops));
  const { tree, skipped } = applyProfile(defaults(), { ops, locked: false });
  assert.equal(skipped.length, 0, skipped.join('; '));
  assert.deepEqual((tree.children?.[0] as unknown as Record<string, unknown>)['on'], { click: { do: 'ui.goBack' } });
  assert.deepEqual((tree.children?.[0] as unknown as Record<string, unknown>)['bind'], { title: 'store.mode' });
  assert.equal((tree.children?.[1] as unknown as Record<string, unknown>)['when'], 'store.locked');
}

console.log('profile.test.ts: ok');

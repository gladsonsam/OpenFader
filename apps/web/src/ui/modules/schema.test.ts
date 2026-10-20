// L0 tests: schema validation (valid/invalid/lenient), repeat expansion,
// `when` conditions, device-class variant selection.
import { strict as assert } from 'node:assert';
import {
  applyVariant,
  countNodes,
  evalWhen,
  expandRepeat,
  findNode,
  findParent,
} from './schema.ts';
import { normalizeLayout, validateLayout, validateNode } from './validate.ts';

const known = (id: string): boolean => ['text', 'row', 'col'].includes(id);

// --- valid layout passes ---
{
  const tree = {
    id: 'root',
    module: 'col',
    props: { gap: 8 },
    children: [
      { id: 't1', module: 'text', props: { value: 'hi' } },
      { id: 'r1', module: 'row', children: [{ id: 't2', module: 'text' }] },
    ],
  };
  const r = validateLayout(tree);
  assert.equal(r.ok, true, JSON.stringify(r.errors));
}

// --- invalid layouts produce errors, never throw ---
{
  const cases: unknown[] = [
    null,
    42,
    'x',
    {},
    { id: '', module: '' },
    { id: 'a', module: 'row', children: [{ id: 'a', module: 'text' }, { id: 'a', module: 'text' }] },
    { id: 'a', module: 'repeat', children: [] },
    { id: 'a', module: 'text', props: 'nope' },
    { id: 'a', module: 'text', bind: { p: 42 } },
    { id: 'a', module: 'text', when: { device: 'watch' } },
    { id: 'a', module: 'text', variants: { watch: {} } },
  ];
  for (const c of cases) {
    const r = validateNode(c);
    assert.equal(r.ok, false, `should fail: ${JSON.stringify(c)}`);
    assert.ok(r.errors.length > 0);
  }
}

// --- lenient normaliser: unknown modules become visible placeholders ---
{
  const { tree, fixed } = normalizeLayout(
    {
      id: 'root',
      module: 'col',
      children: [
        { id: 'good', module: 'text', props: { value: 'x' } },
        { id: 'weird', module: 'fancy-fader-3000', props: { v: 1 } },
        null,
        { id: 'empty-module' },
      ],
    },
    known,
  );
  assert.equal(tree.module, 'col');
  const kids = tree.children ?? [];
  assert.equal(kids.length, 3);
  assert.equal(kids[0].module, 'text');
  assert.equal(kids[1].module, 'missing-module');
  assert.equal(kids[1].props?.['missing'], 'fancy-fader-3000');
  assert.equal(kids[2].module, 'missing-module');
  assert.ok(fixed.length >= 3, fixed.join('; '));
  // Garbage root still yields a renderable tree, never throws.
  const g = normalizeLayout('nope', known);
  assert.equal(g.tree.module, 'missing-module');
  // Editor flags survive normalisation (hide ops apply through the renderer).
  const h = normalizeLayout({ id: 'r', module: 'col', hidden: true, children: [] }, known);
  assert.equal(h.tree.hidden, true);
}

// --- repeat expansion over fixtures ---
{
  const node = {
    id: 'rep',
    module: 'repeat',
    props: { over: 'strips' },
    children: [{ id: 't', module: 'text', props: { value: 'strip {item}' } }],
  };
  const scope = { strips: ['in1', 'in2', 'in3'], bankItems: ['in1'], lists: { demo: ['a', 'b'] } };
  const out = expandRepeat(node, scope);
  assert.equal(out.length, 3);
  assert.deepEqual(out.map(n => n.id), ['rep[in1]', 'rep[in2]', 'rep[in3]']);
  assert.equal(out[0].props?.['item'], 'in1');
  assert.equal(out[0].bind?.['stripId'], 'strip:in1');
  // bank + list overs
  const banked = expandRepeat({ ...node, props: { over: 'bank:ch1' } }, scope);
  assert.deepEqual(banked.map(n => n.id), ['rep[in1]']);
  const listed = expandRepeat({ ...node, props: { over: 'list:demo' } }, scope);
  assert.deepEqual(listed.map(n => n.id), ['rep[a]', 'rep[b]']);
  // non-repeat passes through; empty template expands to nothing
  assert.deepEqual(expandRepeat({ id: 'x', module: 'row' }, scope).length, 1);
  assert.deepEqual(expandRepeat({ id: 'y', module: 'repeat', props: {} }, scope), []);
}

// --- when conditions ---
{
  const ctx = { device: 'desktop' as const, role: 'engineer' as const, caps: ['eq_on'], settings: { sof: true } };
  assert.equal(evalWhen(undefined, ctx), true);
  assert.equal(evalWhen({ device: 'desktop' }, ctx), true);
  assert.equal(evalWhen({ device: 'phone' }, ctx), false);
  assert.equal(evalWhen({ role: ['musician', 'engineer'] }, ctx), true);
  assert.equal(evalWhen({ role: 'musician' }, ctx), false);
  assert.equal(evalWhen({ caps: ['eq_on'] }, ctx), true);
  assert.equal(evalWhen({ caps: ['nope'] }, ctx), false);
  assert.equal(evalWhen({ setting: ['setting:sof'] }, ctx), true);
  assert.equal(evalWhen({ setting: ['setting:missing'] }, ctx), false);
  // Unknown context keeps the node (never hide user content on missing data),
  // except caps which are authoritative.
  assert.equal(evalWhen({ device: 'phone' }, {}), false);
  assert.equal(evalWhen({ role: 'musician' }, {}), false);
  assert.equal(evalWhen({ setting: ['setting:sof'] }, {}), false);
  assert.equal(evalWhen({ caps: ['eq_on'] }, {}), false);
}

// --- device-class variants ---
{
  const node = {
    id: 'n',
    module: 'text',
    props: { value: 'desktop note', size: 12 },
    variants: { phone: { props: { value: 'phone note' } } },
  };
  assert.equal(applyVariant(node, 'desktop').props?.['value'], 'desktop note');
  assert.equal(applyVariant(node, 'phone').props?.['value'], 'phone note');
  assert.equal(applyVariant(node, 'phone').props?.['size'], 12);
  assert.equal(applyVariant({ id: 'x', module: 'text' }, 'phone').module, 'text');
}

// --- L1 validators: on/bind/when-string (never throw; warnings keep nodes) ---
{
  // known action + good args: clean
  const good = validateNode({
    id: 'b',
    module: 'btn',
    bind: { label: 'item.label', pressed: "item.on ? 'true' : 'false'", legacy: 'strip' },
    on: { press: { do: 'ui.setPatchSel', args: { id: { $expr: 'item.id' } } } },
    when: 'store.mode == "routing"',
  });
  assert.equal(good.ok, true, JSON.stringify(good.errors));
  assert.deepEqual(good.warnings, []);
  // unknown action id: VISIBLE WARNING, node still ok
  const unk = validateNode({ id: 'u', module: 'btn', on: { press: { do: 'nope.action' } } });
  assert.equal(unk.ok, true);
  assert.equal(unk.warnings.length, 1);
  assert.ok(unk.warnings[0].includes('nope.action'), unk.warnings.join('; '));
  // sequence with one unknown: same, one warning
  const seq = validateNode({
    id: 's', module: 'btn',
    on: { press: [{ do: 'ui.goBack' }, { do: 'nope.x' }] },
  });
  assert.equal(seq.ok, true);
  assert.equal(seq.warnings.length, 1);
  // bad shapes are ERRORS: non-object on, bad ref, bad args, bad exprs
  for (const n of [
    { id: 'x', module: 'btn', on: 'press' },
    { id: 'x', module: 'btn', on: { press: { args: {} } } },
    { id: 'x', module: 'btn', on: { press: { do: 'ui.setFader', args: { id: 'in1', value: 'loud' } } } },
    { id: 'x', module: 'btn', on: { press: { do: 'ui.goBack', args: { q: { $nope: 1 } } } } },
    { id: 'x', module: 'btn', on: { press: { do: 'ui.goBack', args: { q: { $expr: 'nope(' } } } } },
    { id: 'x', module: 'btn', bind: { p: 'a.constructor' } },
    { id: 'x', module: 'btn', bind: { p: 'foo(1)' } },
    { id: 'x', module: 'btn', when: 'store.mode ==' },
    { id: 'x', module: 'btn', on: { '': { do: 'ui.goBack' } } },
    { id: 'x', module: 'btn', on: { press: [] } },
  ]) {
    const r = validateNode(n);
    assert.equal(r.ok, false, `should fail: ${JSON.stringify(n)}: ${JSON.stringify(r.errors)}`);
  }
  // legacy bindings still validate (scenes/settings default layouts rely on it)
  assert.equal(validateNode({ id: 'x', module: 'text', bind: { s: 'scene' } }).ok, true);
  assert.equal(validateNode({ id: 'x', module: 'text', bind: { s: 'setting:sof' } }).ok, true);
  // normaliser preserves on + string-when and keeps unknown-action nodes
  const norm = normalizeLayout(
    {
      id: 'root', module: 'col',
      children: [{ id: 'b', module: 'btn', bind: { l: 'item.x' }, on: { press: { do: 'nope.x' } }, when: 'store.ok' }],
    },
    (id) => id === 'btn',
  );
  assert.equal((norm.tree.children?.[0].on?.['press'] as unknown as { do: string }).do, 'nope.x');
  assert.equal(norm.tree.children?.[0].bind?.['l'], 'item.x');
  assert.equal(norm.tree.children?.[0].when, 'store.ok');
}

// --- find/count helpers ---
{
  const tree = {
    id: 'root',
    module: 'col',
    children: [{ id: 'a', module: 'row', children: [{ id: 'b', module: 'text' }] }],
  };
  assert.equal(countNodes(tree), 3);
  assert.equal(findNode(tree, 'b')?.module, 'text');
  assert.equal(findParent(tree, 'b')?.id, 'a');
  assert.equal(findParent(tree, 'root'), undefined);
}

console.log('schema.test.ts: ok');

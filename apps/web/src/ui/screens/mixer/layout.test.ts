// W5c Mixer default-layout test: mixer.default.json is a valid layout tree
// whose composition, bindings and actions drive the screen (no Svelte
// composition). Every module is a container or a registered mixer leaf, every
// action id is known, every bind/when/$expr parses, and the strip repeat
// template is exactly strip.default.json (the user-editable strip anatomy).
// Run: node --experimental-strip-types apps/web/src/ui/screens/mixer/layout.test.ts
import { strict as assert } from 'node:assert';
import { readFileSync } from 'node:fs';
import { CONTAINERS, type LayoutNode } from '../../modules/schema.ts';
import { validateLayout } from '../../modules/validate.ts';
import '../../modules/actions.ts';
import './actions.ts';

const mixer = JSON.parse(
  readFileSync(new URL('../../layouts/mixer.default.json', import.meta.url), 'utf8'),
) as LayoutNode;
const strip = JSON.parse(
  readFileSync(new URL('../../layouts/strip.default.json', import.meta.url), 'utf8'),
) as LayoutNode;

const LEAVES = new Set([
  'strip-frame',
  'strip-pan',
  'strip-eq-mini',
  'strip-dyn-mini',
  'strip-mute',
  'strip-fader',
  'strip-name',
  'strip-empty',
  'strip-well-empty',
]);
const KNOWN_ON: Record<string, Set<string>> = {
  'strip-pan': new Set(['input']),
  'strip-eq-mini': new Set(['open']),
  'strip-dyn-mini': new Set(['open']),
  'strip-mute': new Set(['press']),
  'strip-fader': new Set(['input']),
  'strip-name': new Set(['select']),
};

function walk(n: LayoutNode, out: LayoutNode[]): LayoutNode[] {
  out.push(n);
  for (const c of n.children ?? []) walk(c, out);
  return out;
}

{
  const v = validateLayout(mixer);
  assert.deepEqual(v.errors, [], `mixer layout invalid: ${v.errors.join('; ')}`);
  assert.deepEqual(v.warnings, [], `mixer unknown actions: ${v.warnings.join('; ')}`);
  assert.ok(v.ok);
  const vs = validateLayout(strip);
  assert.deepEqual(vs.errors, [], `strip layout invalid: ${vs.errors.join('; ')}`);
  assert.deepEqual(vs.warnings, [], `strip unknown actions: ${vs.warnings.join('; ')}`);
  assert.ok(vs.ok);
}

{
  // root is a transparent flex row (shell body provides padding/gap): the
  // scroll row + the master section are its only children.
  assert.equal(mixer.module, 'row');
  assert.deepEqual((mixer.children ?? []).map(c => c.id), ['strip-scroll', 'master']);
  const nodes = walk(mixer, []);
  for (const n of nodes) {
    assert.ok(CONTAINERS.has(n.module) || LEAVES.has(n.module), `unknown module "${n.module}" at ${n.id}`);
  }
  // every on.* event is emitted by its module; sequences are non-empty.
  for (const n of nodes) {
    for (const [ev, target] of Object.entries(n.on ?? {})) {
      assert.ok(KNOWN_ON[n.module]?.has(ev), `${n.id} (${n.module}): unknown event ${ev}`);
      for (const s of Array.isArray(target) ? target : [target]) {
        assert.ok(typeof (s as { do: string }).do === 'string');
      }
    }
  }
}

{
  // the strip row is a repeat over bank.visible; trailing empty slots are a
  // second repeat (grid auto-placement keeps the 16-cell order).
  const grid = walk(mixer, []).find(n => n.id === 'strip-grid')!;
  assert.equal(grid.module, 'grid');
  assert.equal(grid.bind?.['columns'], 'setting:gridCols');
  assert.deepEqual((grid.children ?? []).map(c => c.id), ['strips', 'empties']);
  const [strips, empties] = grid.children as LayoutNode[];
  assert.equal(strips.module, 'repeat');
  assert.equal((strips.props as Record<string, unknown>)?.['over'], 'bank:visible');
  assert.equal(strips.children?.length, 1);
  assert.equal(empties.module, 'repeat');
  assert.equal((empties.props as Record<string, unknown>)?.['over'], 'list:emptySlots');
  assert.equal(empties.children?.length, 1);
  assert.equal((empties.children as LayoutNode[])[0].module, 'strip-empty');
  // the strip template is a frame re-rooting the user-editable strip anatomy
  // (strip.default.json) per cell: the L1 repeat exposes `item` on the
  // template instance only, and the anatomy subtree needs it a level deeper.
  const frame = strips.children?.[0] as LayoutNode;
  assert.equal(frame.module, 'strip-frame');
  assert.deepEqual(frame.bind, {
    cell: 'item',
    index: 'index',
    store: 'store',
    frozen: 'frozen',
    tree: 'anatomy',
    device: 'device',
  });
  // the anatomy itself is a valid strip tree (pan, proc well, mute,
  // fader, name) rendered by the frame through the real LayoutRenderer.
  assert.equal(strip.module, 'col');
  assert.deepEqual(
    (strip.children ?? []).map(c => c.id),
    ['pan', 'proc', 'proc-empty', 'mute', 'fader-area', 'name'],
  );
}

{
  // master is its own landmark section (always-rendered proc well, master
  // fader, SOF-aware accent) bound to the scope master cell.
  const master = walk(mixer, []).find(n => n.id === 'master')!;
  assert.equal((master.props as Record<string, unknown>)?.['tag'], 'section');
  const ids = walk(master, []).map(n => n.id);
  for (const id of ['master-proc', 'master-eq-mini', 'master-dyn-mini', 'master-mute', 'master-fader', 'master-name']) {
    assert.ok(ids.includes(id), `master missing ${id}`);
  }
  const fader = walk(master, []).find(n => n.id === 'master-fader')!;
  assert.equal((fader.props as Record<string, unknown>)?.['variant'], 'master');
  assert.equal(fader.bind?.['accent'], "master.kind == 'sof' ? 'sof' : 'master'");
}

console.log('mixer layout.test.ts: ok');

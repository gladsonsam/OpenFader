// L1 Routing default-layout test: routing.default.json is a valid layout tree
// whose composition, bindings and actions drive the screen (no Svelte
// composition). Every module is a container or a registered leaf, every
// action id is known, every bind/when/$expr parses.
// Run: node --experimental-strip-types apps/web/src/ui/screens/routing/layout.test.ts
import { strict as assert } from 'node:assert';
import { readFileSync } from 'node:fs';
import { CONTAINERS, type LayoutNode } from '../../modules/schema.ts';
import { validateLayout } from '../../modules/validate.ts';

const tree = JSON.parse(
  readFileSync(new URL('../../layouts/routing.default.json', import.meta.url), 'utf8'),
) as LayoutNode;

const LEAVES = new Set(['btn', 'route-matrix', 'text']);
const KNOWN_ON = new Set(['press']);

function walk(n: LayoutNode, out: LayoutNode[]): LayoutNode[] {
  out.push(n);
  for (const c of n.children ?? []) walk(c, out);
  return out;
}

{
  const v = validateLayout(tree);
  assert.deepEqual(v.errors, [], `layout invalid: ${v.errors.join('; ')}`);
  assert.deepEqual(v.warnings, [], `unknown actions: ${v.warnings.join('; ')}`);
  assert.ok(v.ok);
}

{
  assert.equal(tree.module, 'col');
  const nodes = walk(tree, []);
  for (const n of nodes) {
    assert.ok(CONTAINERS.has(n.module) || LEAVES.has(n.module), `unknown module "${n.module}" at ${n.id}`);
  }
  const ids = nodes.map(n => n.id);
  assert.ok(ids.includes('patchbay') && ids.includes('mx-matrix') && ids.includes('mx-scroll'), ids.join(','));
  // repeats own exactly one template and read screen lists
  const reps = nodes.filter(n => n.module === 'repeat');
  assert.deepEqual(reps.map(r => r.id).sort(), ['mx-groups']);
  for (const r of reps) {
    assert.equal(r.children?.length, 1, r.id);
    assert.ok(String((r.props as Record<string, unknown>)?.['over']).startsWith('list:'), r.id);
  }
  // every on.* event is a known leaf event; sequences are non-empty
  for (const n of nodes) {
    for (const [ev, target] of Object.entries(n.on ?? {})) {
      assert.ok(KNOWN_ON.has(ev), `${n.id}: unknown event ${ev}`);
      for (const s of Array.isArray(target) ? target : [target]) {
        assert.ok(typeof (s as { do: string }).do === 'string');
      }
    }
  }
  // sections keep landmark labels + content-box cards
  const secs = nodes.filter(n => (n.props as Record<string, unknown> | undefined)?.['tag'] === 'section');
  assert.deepEqual(secs.map(s => (s.props as Record<string, unknown>)['label']).sort(), ['Patchbay']);
  // the matrix toggles the route for that channel+source (no bank jumping)
  const matrix = nodes.find(n => n.id === 'mx-matrix');
  assert.equal(matrix?.module, 'route-matrix');
  assert.equal((matrix?.on?.['press'] as { do: string } | undefined)?.do, 'ui.toggleRoute');
  // presets stage (arm) instead of patching directly; Apply commits
  const preset = nodes.find(n => n.id === 'mx-preset');
  assert.equal((preset?.on?.['press'] as { do: string } | undefined)?.do, 'ui.armPatch');
  const apply = nodes.find(n => n.id === 'mx-apply');
  assert.equal((apply?.on?.['press'] as { do: string } | undefined)?.do, 'ui.applyPatch');
  assert.ok(typeof apply?.when === 'string' && apply.when.length > 0, 'apply gated on staged preset');
  const cancel = nodes.find(n => n.id === 'mx-cancel');
  assert.equal((cancel?.on?.['press'] as { do: string } | undefined)?.do, 'ui.cancelPatch');
  assert.ok(secs.every(s => (s.props as Record<string, unknown>)['box'] === 'content-box'));
}

console.log('routing layout.test.ts: ok');

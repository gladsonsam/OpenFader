// W5a Scenes default-layout test: scenes.default.json is a valid layout
// tree whose card grid is a repeat over the scene card vms (sceneCards in
// scope order) and whose recall bar binds the recall vm. select dispatches
// ui.pickScene; cancel/recall dispatch ui.goBack/ui.recallScene.
// Run: node --experimental-strip-types apps/web/src/ui/screens/scenes/layouts.test.ts

import { strict as assert } from 'node:assert';
import { readFileSync } from 'node:fs';
import { CONTAINERS, type LayoutNode } from '../../modules/schema.ts';
import { validateLayout } from '../../modules/validate.ts';
import '../../modules/actions.ts';

const tree = JSON.parse(
  readFileSync(new URL('../../layouts/scenes.default.json', import.meta.url), 'utf8'),
) as LayoutNode;

{
  const v = validateLayout(tree);
  assert.deepEqual(v.errors, [], `layout invalid: ${v.errors.join('; ')}`);
  assert.deepEqual(v.warnings, [], `unknown actions: ${v.warnings.join('; ')}`);
  assert.ok(v.ok);
}

function walk(n: LayoutNode, out: LayoutNode[]): LayoutNode[] {
  out.push(n);
  for (const c of n.children ?? []) walk(c, out);
  return out;
}
const nodes = walk(tree, []);
const KNOWN = new Set(['scene-card', 'scene-recall-bar']);
for (const n of nodes) {
  assert.ok(CONTAINERS.has(n.module) || KNOWN.has(n.module), `unknown module "${n.module}" at ${n.id}`);
}

// Root col + scroll (flex + min-height 0 so the grid scrolls and the bar
// stays put) + grid + one repeat over the card list.
{
  assert.equal(tree.module, 'col');
  assert.equal((tree.props as Record<string, unknown>)['gap'], 8);
  const scroll = (tree.children ?? [])[0];
  assert.equal(scroll.module, 'scroll');
  assert.equal((scroll.props as Record<string, unknown>)['flex'], 1);
  assert.equal((scroll.props as Record<string, unknown>)['minHeight'], 0);
  const grid = (scroll.children ?? [])[0];
  assert.equal(grid.module, 'grid');
  assert.equal((grid.props as Record<string, unknown>)['columns'], 'repeat(auto-fill, minmax(220px, 1fr))');
  assert.equal((grid.props as Record<string, unknown>)['gap'], 6);
  const rep = (grid.children ?? [])[0];
  assert.equal(rep.module, 'repeat');
  assert.equal((rep.props as Record<string, unknown>)['over'], 'list:sceneCards');
  assert.equal(rep.children?.length, 1);
  assert.equal(rep.children?.[0].module, 'scene-card');
}

// Card binds one card vm and dispatches ui.pickScene with the payload index.
{
  const card = nodes.find(n => n.module === 'scene-card');
  assert.ok(card);
  assert.deepEqual(card.bind, {
    index: 'item.index',
    num: 'item.num',
    name: 'item.name',
    tag: 'item.tag',
    pressed: 'item.pressed',
    border: 'item.border',
    bg: 'item.bg',
  });
  assert.deepEqual(card.on, {
    select: { do: 'ui.pickScene', args: { index: { $expr: 'payload.index' } } },
  });
}

// Recall bar binds the recall vm; cancel/recall dispatch goBack/recallScene.
{
  const bar = nodes.find(n => n.module === 'scene-recall-bar');
  assert.ok(bar);
  assert.deepEqual(bar.bind, {
    tag: 'recall.tag',
    tagColor: 'recall.tagColor',
    num: 'recall.num',
    name: 'recall.name',
    barBorder: 'recall.barBorder',
    border: 'recall.border',
    bg: 'recall.bg',
    text: 'recall.text',
    disabled: 'recall.disabled',
  });
  assert.deepEqual(bar.on, {
    cancel: { do: 'ui.goBack' },
    recall: { do: 'ui.recallScene' },
  });
}

console.log('scenes layouts.test.ts: ok');

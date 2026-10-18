// W5a FX default-layout test: fx.default.json is a valid layout tree whose
// slot row is a grid + repeat over the slot vms (fxSlots in rack order) and
// whose detail area is one fx-detail-panel reading the selected slot. Type
// buttons are a repeat of buttons inside the panel (pickType ->
// ui.setFxType); knob drags commit knob/reset -> ui.setFxParam/ui.resetFxParam.
// Run: node --experimental-strip-types apps/web/src/ui/screens/fx/layout.test.ts
import { strict as assert } from 'node:assert';
import { readFileSync } from 'node:fs';
import { CONTAINERS, type LayoutNode } from '../../modules/schema.ts';
import { validateLayout } from '../../modules/validate.ts';
import '../../modules/actions.ts';

const tree = JSON.parse(
  readFileSync(new URL('../../layouts/fx.default.json', import.meta.url), 'utf8'),
) as LayoutNode;

{
  const v = validateLayout(tree);
  assert.deepEqual(v.errors, [], `layout invalid: ${v.errors.join('; ')}`);
  assert.deepEqual(v.warnings, [], `unknown actions: ${v.warnings.join('; ')}`);
  assert.ok(v.ok);
}

const LEAVES = new Set(['fx-slot-card', 'fx-detail-panel']);
const KNOWN_ON = new Set(['select', 'toggle', 'tap', 'pickType', 'knob', 'reset']);

function walk(n: LayoutNode, out: LayoutNode[]): LayoutNode[] {
  out.push(n);
  for (const c of n.children ?? []) walk(c, out);
  return out;
}

{
  assert.equal(tree.module, 'col');
  const nodes = walk(tree, []);
  for (const n of nodes) {
    assert.ok(CONTAINERS.has(n.module) || LEAVES.has(n.module), `unknown module "${n.module}" at ${n.id}`);
  }
  const ids = nodes.map(n => n.id);
  for (const id of ['slots', 'slot-rep', 'slot-card', 'detail']) {
    assert.ok(ids.includes(id), `missing node ${id} in ${ids.join(',')}`);
  }
  // every on.* event is a known leaf event with a do action
  for (const n of nodes) {
    for (const [ev, target] of Object.entries(n.on ?? {})) {
      assert.ok(KNOWN_ON.has(ev), `${n.id}: unknown event ${ev}`);
      for (const s of Array.isArray(target) ? target : [target]) {
        assert.ok(typeof (s as { do: string }).do === 'string');
      }
    }
  }
}

// Slot grid: auto-fit minmax 140px, gap 6; one repeat over fxSlots; the card
// binds one slot vm and dispatches ui.setFxSel with the payload index.
{
  const nodes = walk(tree, []);
  const grid = nodes.find(n => n.id === 'slots');
  assert.equal(grid?.module, 'grid');
  assert.equal((grid?.props as Record<string, unknown>)?.['columns'], 'repeat(auto-fit, minmax(140px, 1fr))');
  assert.equal((grid?.props as Record<string, unknown>)?.['gap'], 6);
  const rep = nodes.find(n => n.id === 'slot-rep');
  assert.equal(rep?.module, 'repeat');
  assert.equal((rep?.props as Record<string, unknown>)?.['over'], 'list:fxSlots');
  assert.equal(rep?.children?.length, 1);
  const card = nodes.find(n => n.id === 'slot-card');
  assert.equal(card?.module, 'fx-slot-card');
  assert.deepEqual(card?.bind, {
    index: 'item.index',
    name: 'item.name',
    type: 'item.type',
    state: 'item.state',
    stateColor: 'item.stateColor',
    ret: 'item.ret',
    pressed: 'item.pressed',
    border: 'item.border',
    bg: 'item.bg',
    meterPct: 'item.meterPct',
  });
  assert.deepEqual(card?.on, {
    select: { do: 'ui.setFxSel', args: { index: { $expr: 'payload.index' } } },
  });
}

// Detail panel reads the selected slot + knob descriptor list + fine/locked;
// every event maps to a ui.* FX action with store.fxSel / payload refs.
{
  const nodes = walk(tree, []);
  const detail = nodes.find(n => n.id === 'detail');
  assert.equal(detail?.module, 'fx-detail-panel');
  assert.deepEqual(detail?.bind, {
    title: 'fx.title',
    toggle: 'fx.toggle',
    isDelay: 'fx.isDelay',
    tapLabel: 'fx.tapLabel',
    types: 'fx.types',
    slot: 'slot',
    knobs: 'knobs',
    fine: 'store.fine',
    locked: 'store.locked',
  });
  assert.deepEqual(detail?.on, {
    toggle: { do: 'ui.toggleFxOn', args: { index: { $expr: 'store.fxSel' } } },
    tap: { do: 'ui.tapFx', args: { index: { $expr: 'store.fxSel' } } },
    pickType: {
      do: 'ui.setFxType',
      args: { index: { $expr: 'store.fxSel' }, type: { $expr: 'payload.type' } },
    },
    knob: {
      do: 'ui.setFxParam',
      args: {
        index: { $expr: 'store.fxSel' },
        param: { $expr: 'payload.param' },
        value: { $expr: 'payload.value' },
      },
    },
    reset: {
      do: 'ui.resetFxParam',
      args: { index: { $expr: 'store.fxSel' }, param: { $expr: 'payload.param' } },
    },
  });
}

console.log('fx layout.test.ts: ok');

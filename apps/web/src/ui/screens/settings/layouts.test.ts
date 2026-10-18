// W5a Settings default-layout test: settings.default.json is a valid layout
// tree whose panels/rows stay in sync with SETTING_DESCRIPTORS. The rows are
// data-driven: each panel repeats over its descriptor-derived row list in
// scope (displayRows/faderRows/safetyRows/aboutRows), and each row binds one
// descriptor row and emits change -> ui.setSetting.
// Run: node --experimental-strip-types apps/web/src/ui/screens/settings/layouts.test.ts

import { strict as assert } from 'node:assert';
import { readFileSync } from 'node:fs';
import { CONTAINERS, type LayoutNode } from '../../modules/schema.ts';
import { validateLayout } from '../../modules/validate.ts';
import '../../modules/actions.ts';
import './actions.ts';
import { SETTING_DESCRIPTORS } from './settingsVm.ts';

const tree = JSON.parse(
  readFileSync(new URL('../../layouts/settings.default.json', import.meta.url), 'utf8'),
) as LayoutNode;

{
  const v = validateLayout(tree);
  assert.deepEqual(v.errors, [], `layout invalid: ${v.errors.join('; ')}`);
  assert.deepEqual(v.warnings, [], `unknown actions: ${v.warnings.join('; ')}`);
  assert.ok(v.ok);
}

function walk(n: LayoutNode, out: LayoutNode[]): void {
  out.push(n);
  for (const c of n.children ?? []) walk(c, out);
}
const nodes: LayoutNode[] = [];
walk(tree, nodes);
const KNOWN = new Set(['setting-row', 'settings-panel']);
for (const n of nodes) {
  assert.ok(CONTAINERS.has(n.module) || KNOWN.has(n.module), `unknown module "${n.module}" at ${n.id}`);
}

// Centred 1100px wrapper (the design template's grid) is owned by the layout.
{
  assert.equal(tree.module, 'grid');
  const p = tree.props as Record<string, unknown>;
  assert.equal(p['columns'], 'repeat(auto-fit, minmax(380px, 1fr))');
  assert.equal(p['gap'], 8);
  assert.equal(p['align'], 'start');
  assert.equal(p['maxWidth'], 1100);
  assert.equal(p['margin'], '0 auto');
  assert.equal(p['box'], 'content-box');
}

// Four panels in descriptor order, each with one repeat over its row list.
{
  const panels = tree.children ?? [];
  assert.deepEqual(panels.map(p => (p.props as Record<string, unknown>)['title']), ['Display', 'Faders', 'Safety', 'About']);
  assert.ok(panels.every(p => p.module === 'settings-panel'));
  const overs = panels.map(p =>
    String((p.children?.[0].props as Record<string, unknown>)?.['over'] ?? ''),
  );
  assert.deepEqual(overs, ['list:displayRows', 'list:faderRows', 'list:safetyRows', 'list:aboutRows']);
  for (const p of panels) {
    assert.equal(p.children?.length, 1);
    assert.equal(p.children?.[0].module, 'repeat');
    assert.equal(p.children?.[0].children?.length, 1);
    assert.equal(p.children?.[0].children?.[0].module, 'setting-row');
  }
}

// Every row binds one descriptor row and dispatches ui.setSetting with
// payload {key, value}; every descriptor key is covered by exactly one row.
{
  const rows = nodes.filter(n => n.module === 'setting-row');
  assert.equal(rows.length, 4, 'one row template per panel repeat');
  for (const r of rows) {
    assert.deepEqual(r.bind, {
      rowKey: 'item.key',
      label: 'item.label',
      desc: 'item.desc',
      control: 'item.control',
      options: 'item.options',
    });
    const targets = Array.isArray(r.on?.['change']) ? r.on?.['change'] : [r.on?.['change']];
    assert.equal(targets.length, 1);
    const t = targets[0] as { do: string; args: Record<string, unknown> };
    assert.equal(t.do, 'ui.setSetting');
    assert.deepEqual(t.args, { key: { $expr: 'payload.key' }, value: { $expr: 'payload.value' } });
  }
  const panels = tree.children ?? [];
  const overs = panels.map(p => String((p.children?.[0].props as Record<string, unknown>)?.['over']?.slice(5)));
  const byPanel: Record<string, string[]> = {};
  for (const d of SETTING_DESCRIPTORS) {
    const list = d.panel === 'Display' ? 'displayRows' : d.panel === 'Faders' ? 'faderRows' : d.panel === 'Safety' ? 'safetyRows' : 'aboutRows';
    (byPanel[list] ??= []).push(d.key);
  }
  assert.deepEqual(overs, Object.keys(byPanel));
  assert.ok(Object.values(byPanel).every(keys => keys.length > 0), 'every panel repeat covers >= 1 descriptor');
  assert.equal(Object.values(byPanel).flat().length, SETTING_DESCRIPTORS.length);
}

console.log('settings layouts.test.ts: ok');

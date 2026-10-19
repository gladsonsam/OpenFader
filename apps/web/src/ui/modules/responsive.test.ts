// Responsive phone/tablet variants: the device-class overlays in the shipped
// default layouts resolve through the renderer path (applyVariant + scope
// bindings) without changing the desktop tree, and every modified layout
// still validates with no unknown actions.
// Run: node --experimental-strip-types apps/web/src/ui/modules/responsive.test.ts
import { strict as assert } from 'node:assert';
import { readFileSync } from 'node:fs';
import { applyVariant, findNode, type LayoutNode } from './schema.ts';
import { validateLayout } from './validate.ts';
import { evalExpr } from './expr.ts';
import { deviceClassOf } from './device.ts';
import { phoneGridCols, tabletGridCols } from '../screens/mixer/mixerVm.ts';
import './actions.ts';
import '../screens/mixer/actions.ts';
import '../screens/channel/actions.ts';
import '../screens/settings/actions.ts';

function load(rel: string): LayoutNode {
  return JSON.parse(readFileSync(new URL(rel, import.meta.url), 'utf8')) as LayoutNode;
}

const mixer = load('../layouts/mixer.default.json');
const strip = load('../layouts/strip.default.json');
const channel = load('../layouts/channel.default.json');
const settings = load('../layouts/settings.default.json');

for (const [name, tree] of [['mixer', mixer], ['strip', strip], ['channel', channel], ['settings', settings]] as const) {
  const v = validateLayout(tree);
  assert.deepEqual(v.errors, [], `${name} layout invalid: ${v.errors.join('; ')}`);
  assert.deepEqual(v.warnings, [], `${name} unknown actions: ${v.warnings.join('; ')}`);
  assert.ok(v.ok, name);
}

function node(root: LayoutNode, id: string): LayoutNode {
  const n = findNode(root, id);
  assert.ok(n, `missing node ${id}`);
  return n!;
}

// Desktop (1280x720 diff viewport) applies no variant: the tree is untouched.
assert.equal(deviceClassOf(1280), 'desktop');
assert.deepEqual(applyVariant(node(mixer, 'strip-grid'), 'desktop'), node(mixer, 'strip-grid'));
assert.equal(node(mixer, 'strip-grid').bind?.['columns'], 'setting:gridCols');

// Mixer phone/tablet: snapping scroller + fixed pager columns from scope.
assert.equal(deviceClassOf(390), 'phone');
assert.equal(deviceClassOf(834), 'tablet');
const gridPhone = applyVariant(node(mixer, 'strip-grid'), 'phone');
assert.equal(gridPhone.bind?.['columns'], 'phoneCols');
assert.equal((gridPhone.props as Record<string, unknown>)?.['snapItems'], true);
const gridTablet = applyVariant(node(mixer, 'strip-grid'), 'tablet');
assert.equal(gridTablet.bind?.['columns'], 'tabletCols');
const scrollPhone = applyVariant(node(mixer, 'strip-scroll'), 'phone');
assert.equal((scrollPhone.props as Record<string, unknown>)?.['snap'], 'x');
assert.equal((applyVariant(node(mixer, 'strip-scroll'), 'desktop').props as Record<string, unknown>)?.['snap'], undefined);

// The variant column expressions resolve against the screen scope.
assert.equal(phoneGridCols(), 'repeat(48, 120px)');
assert.equal(tabletGridCols(), 'repeat(16, 150px)');
for (const [cls, cols] of [['phone', phoneGridCols()], ['tablet', tabletGridCols()]] as const) {
  const r = evalExpr(cls === 'phone' ? 'phoneCols' : 'tabletCols', { phoneCols: phoneGridCols(), tabletCols: tabletGridCols() });
  assert.ok(r.ok, `${cls} columns expr failed`);
  assert.equal((r as { ok: true; value: unknown }).value, cols);
}

// The strip frame forwards the renderer device into its nested tree so the
// user-editable strip anatomy applies its own phone/tablet variants.
assert.equal(
  (node(mixer, 'strips').children as LayoutNode[])[0].bind?.['device'],
  'device',
);
// Mute buttons reach the 44px touch minimum on phone/tablet only.
assert.equal((applyVariant(node(strip, 'mute'), 'phone').props as Record<string, unknown>)?.['height'], 48);
assert.equal((applyVariant(node(strip, 'mute'), 'tablet').props as Record<string, unknown>)?.['height'], 48);
assert.equal((node(strip, 'mute').props as Record<string, unknown>)?.['height'], 40);
assert.equal((applyVariant(node(mixer, 'master-mute'), 'phone').props as Record<string, unknown>)?.['height'], 48);
// Phone master strip narrows so two full strips stay visible.
assert.equal((node(mixer, 'master').props as Record<string, unknown>)?.['width'], 112);
assert.equal((applyVariant(node(mixer, 'master'), 'phone').props as Record<string, unknown>)?.['width'], 96);

// Channel phone: the row stacks vertically (rail / content / master).
const chRoot = applyVariant(channel, 'phone');
assert.equal(chRoot.module, 'col');
assert.equal(channel.module, 'row');
const tabsPhone = applyVariant(node(channel, 'tab-list'), 'phone');
assert.equal(tabsPhone.module, 'row');
assert.equal((tabsPhone.props as Record<string, unknown>)?.['overflow'], 'auto');
const contentPhone = applyVariant(node(channel, 'content'), 'phone');
assert.equal((contentPhone.props as Record<string, unknown>)?.['minHeight'], 0);
const masterPhone = applyVariant(node(channel, 'master'), 'phone');
assert.equal(masterPhone.module, 'row');
assert.deepEqual((masterPhone.children ?? []).map(c => c.id), ['m-side']);
const side = masterPhone.children![0];
assert.deepEqual((side.children ?? []).map(c => c.id), ['name', 'pan', 'mute', 'state-icons', 'src', 'm-groups']);
// Copied phone nodes keep their bindings and actions.
assert.equal(node(side, 'pan').bind?.['value'], 'master.pan');
assert.equal((node(side, 'mute').on?.['press'] as { do: string }).do, 'ui.toggleMute');

// Settings phone: single column (desktop keeps the auto-fit grid).
assert.equal(
  (node(settings, 'settings-root').props as Record<string, unknown>)?.['columns'],
  'repeat(auto-fit, minmax(380px, 1fr))',
);
assert.equal(
  (applyVariant(node(settings, 'settings-root'), 'phone').props as Record<string, unknown>)?.['columns'],
  '1fr',
);

console.log('responsive.test.ts: ok');

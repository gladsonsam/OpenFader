// W5b channel layout tests: the six default layouts are valid trees whose
// composition, bindings and actions drive the screens (no Svelte
// composition). channel.default.json embeds the five tab layouts; the
// embedded subtrees must stay identical to the standalone tab JSONs
// (single source of the default look, modulo root id + tab `when`).
// Run: node --experimental-strip-types apps/web/src/ui/screens/channel/layout.test.ts
import { strict as assert } from 'node:assert';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { CONTAINERS, type LayoutNode } from '../../modules/schema.ts';
import { validateLayout } from '../../modules/validate.ts';
import '../../modules/actions.ts';
import { registerChannelActions } from './actions.ts';

registerChannelActions();

const HERE = dirname(fileURLToPath(import.meta.url));
const LAYOUTS = join(HERE, '..', '..', 'layouts');

function load(name: string): LayoutNode {
  return JSON.parse(readFileSync(join(LAYOUTS, name), 'utf8')) as LayoutNode;
}

const channel = load('channel.default.json');
const tabs: Record<string, LayoutNode> = {
  input: load('input.default.json'),
  sends: load('sends.default.json'),
  eq: load('eq.default.json'),
  gate: load('gate.default.json'),
  comp: load('comp.default.json'),
};

const LEAVES = new Set([
  'text',
  'channel-tab-button',
  'channel-name-chip',
  'channel-stepper',
  'channel-pan-bar',
  'channel-mute',
  'channel-fader',
  'channel-readout',
  'channel-source-label',
  'input-preamp-card',
  'input-source-card',
  'input-delay-card',
  'send-row',
  'eq-head',
  'eq-summary',
  'eq-graph',
  'eq-hpf-card',
  'eq-band-card',
  'dyn-summary',
  'dyn-transfer-graph',
  'dyn-knob-panel',
]);

const LEAF_EVENTS: Record<string, Set<string>> = {
  'channel-tab-button': new Set(['pick']),
  'channel-stepper': new Set(['prev', 'next']),
  'channel-pan-bar': new Set(['input']),
  'channel-mute': new Set(['press']),
  'channel-fader': new Set(['input']),
  'input-preamp-card': new Set(['knob', 'phantom', 'invert']),
  'input-source-card': new Set(['patch']),
  'input-delay-card': new Set(['knob', 'toggle']),
  'send-row': new Set(['input', 'togglePre']),
  'eq-head': new Set(['toggle', 'flat']),
  'eq-graph': new Set(['select', 'node']),
  'eq-hpf-card': new Set(['select', 'toggle', 'slope', 'freq']),
  'eq-band-card': new Set(['select', 'toggle', 'type', 'param']),
  'dyn-summary': new Set(['toggle']),
  'dyn-knob-panel': new Set(['knob']),
};

function walk(n: LayoutNode, out: LayoutNode[]): LayoutNode[] {
  out.push(n);
  for (const c of n.children ?? []) walk(c, out);
  return out;
}

// --- every file validates with zero errors and zero unknown actions ---
for (const [name, tree] of Object.entries({ channel, ...tabs })) {
  const v = validateLayout(tree);
  assert.deepEqual(v.errors, [], `${name}: ${v.errors.join('; ')}`);
  assert.deepEqual(v.warnings, [], `${name} unknown actions: ${v.warnings.join('; ')}`);
  assert.ok(v.ok, name);
}

// --- every module is a container or a registered channel leaf ---
{
  const nodes = walk(channel, []);
  for (const t of Object.values(tabs)) walk(t, nodes);
  for (const n of nodes) {
    assert.ok(CONTAINERS.has(n.module) || LEAVES.has(n.module), `unknown module "${n.module}" at ${n.id}`);
  }
}

// --- repeats own exactly one template over a scope list ---
{
  const nodes = walk(channel, []);
  const reps = nodes.filter(n => n.module === 'repeat');
  const overs = reps.map(r => String((r.props as Record<string, unknown>)?.['over'])).sort();
  assert.deepEqual(
    overs,
    ['list:eqBands', 'list:sends', 'list:tabs'].sort(),
    'repeat sources: ' + overs.join(','),
  );
  for (const r of reps) {
    assert.equal(r.children?.length, 1, r.id);
    assert.ok(String((r.props as Record<string, unknown>)?.['over']).startsWith('list:'), r.id);
  }
}

// --- on.* events are known leaf events; action refs are well-formed ---
{
  const nodes = walk(channel, []);
  for (const n of nodes) {
    const allowed = LEAF_EVENTS[n.module];
    for (const [ev, target] of Object.entries(n.on ?? {})) {
      if (allowed) assert.ok(allowed.has(ev), `${n.id} (${n.module}): unknown event ${ev}`);
      for (const s of Array.isArray(target) ? target : [target]) {
        assert.ok(typeof (s as { do: string }).do === 'string', `${n.id}.on.${ev}`);
      }
    }
  }
}

// --- channel shell: nav + content slot + master strip ---
{
  assert.equal(channel.module, 'row');
  const [nav, content, master] = channel.children ?? [];
  assert.equal(nav.id, 'tab-list');
  assert.equal((nav.props as Record<string, unknown>)?.['tag'], 'nav');
  assert.equal((nav.props as Record<string, unknown>)?.['label'], 'Processing');
  assert.equal((nav.props as Record<string, unknown>)?.['width'], 112);
  assert.equal(content.id, 'content');
  assert.equal((content.props as Record<string, unknown>)?.['tag'], 'main');
  assert.equal(master.id, 'master');
  assert.equal((master.props as Record<string, unknown>)?.['tag'], 'aside');
  assert.equal((master.props as Record<string, unknown>)?.['label'], 'Channel strip');
  // content shows exactly one tab layout at a time
  const tabNodes = content.children ?? [];
  assert.equal(tabNodes.length, 5);
  for (const t of tabNodes) {
    assert.ok(typeof t.when === 'string' && (t.when as string).startsWith('store.tab =='), `${t.id} when`);
  }
  // master strip is composed from its rebuildable parts
  const parts = (master.children ?? []).flatMap(c =>
    c.module === 'col' ? (c.children ?? []).map(k => k.module) : [c.module],
  );
  for (const m of [
    'channel-name-chip',
    'channel-stepper',
    'channel-pan-bar',
    'channel-mute',
    'channel-fader',
    'channel-source-label',
  ]) {
    assert.ok(parts.includes(m), `master strip has ${m}: ` + parts.join(','));
  }
}

// --- embedded tab subtrees equal the standalone tab JSONs (modulo id/when) ---
function stripRoot(n: LayoutNode): LayoutNode {
  const c = JSON.parse(JSON.stringify(n)) as LayoutNode;
  c.id = '<root>';
  delete (c as Record<string, unknown>)['when'];
  return c;
}
{
  const contentKids = channel.children?.[1]?.children ?? [];
  const byTab: Record<string, string> = {
    'tab-input': 'input',
    'tab-sends': 'sends',
    'tab-eq': 'eq',
    'tab-gate': 'gate',
    'tab-comp': 'comp',
  };
  for (const kid of contentKids) {
    const name = byTab[kid.id];
    assert.ok(name, `unexpected content child ${kid.id}`);
    const solo = tabs[name];
    // standalone roots carry no tab `when`; embedded roots add exactly one
    assert.equal(solo.when, undefined, `${name} standalone root when`);
    assert.deepEqual(stripRoot(kid), stripRoot(solo), `${name}: embedded != standalone`);
  }
}

console.log('channel layout.test.ts: ok');

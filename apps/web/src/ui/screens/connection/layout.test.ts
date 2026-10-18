// L1 Connection default-layout test: connection.default.json is a valid layout
// tree driving the screen (no Svelte composition). Same contract as routing.
// Run: node --experimental-strip-types apps/web/src/ui/screens/connection/layout.test.ts
import { strict as assert } from 'node:assert';
import { readFileSync } from 'node:fs';
import { CONTAINERS, type LayoutNode } from '../../modules/schema.ts';
import { validateLayout } from '../../modules/validate.ts';

const tree = JSON.parse(
  readFileSync(new URL('../../layouts/connection.default.json', import.meta.url), 'utf8'),
) as LayoutNode;

const LEAVES = new Set(['btn', 'status-dot', 'device-row', 'relay-card', 'text']);

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
  // disconnect is conditional on the live state (string-when demo)
  const disc = nodes.find(n => n.id === 'disconnect');
  assert.equal(disc?.when, 'conn.isConnected');
  // device rows repeat over the connection vm list; key flows via payload
  const reps = nodes.filter(n => n.module === 'repeat');
  assert.deepEqual(reps.map(r => r.id), ['dev-rows']);
  assert.equal((reps[0].props as Record<string, unknown>)['over'], 'list:devices');
  const row = reps[0].children?.[0];
  assert.equal(row?.module, 'device-row');
  assert.deepEqual(row?.bind, { device: 'item' });
  assert.deepEqual(row?.on, { connect: { do: 'ui.connect', args: { key: { $expr: 'payload.key' } } } });
  // status border is computed, title/sub interpolate the vm
  const status = nodes.find(n => n.id === 'status');
  assert.deepEqual(status?.bind, { border: "'1px solid ' + conn.cardBorder" });
  // relay card carries the real socket: host in, connect/disconnect out.
  // Phone/tablet only (desktop stays design-exact for the diff baselines).
  const relay = nodes.find(n => n.id === 'relay');
  assert.equal(relay?.module, 'relay-card');
  assert.deepEqual(relay?.when, { device: ['phone', 'tablet'] });
  assert.deepEqual(relay?.bind, {
    host: 'relay.host',
    phase: 'relay.phase',
    message: 'relay.message',
    latencyMs: 'relay.latencyMs',
    strips: 'relay.strips',
  });
  assert.deepEqual(relay?.on, {
    connect: { do: 'ui.connectRelay', args: { host: { $expr: 'payload.host' } } },
    disconnect: { do: 'ui.disconnectRelay' },
  });
}

console.log('connection layout.test.ts: ok');

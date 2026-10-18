// Live relay helper tests (pure rules; the runes singleton is browser-only).
// Run: node --experimental-strip-types apps/web/src/ui/state/live.test.ts
import assert from 'node:assert/strict';
import {
  CONNECT_TIMEOUT_MS,
  DEFAULT_RELAY_HOST,
  linearToDb,
  loadRelayAuto,
  relayHostFromSearch,
  relayUrlFromHost,
} from './live.ts';

// ---- relayUrlFromHost (mirrors transport.ts wsUrlFromQueryString) ----
assert.equal(relayUrlFromHost('127.0.0.1:8080'), 'ws://127.0.0.1:8080/ws');
assert.equal(relayUrlFromHost('192.168.1.50:8080'), 'ws://192.168.1.50:8080/ws');
assert.equal(relayUrlFromHost('  192.168.1.50:8080  '), 'ws://192.168.1.50:8080/ws');
assert.equal(relayUrlFromHost(''), 'ws://127.0.0.1:8080/ws');
assert.equal(relayUrlFromHost('ws://a:1/ws'), 'ws://a:1/ws');
assert.equal(relayUrlFromHost('wss://a/ws'), 'wss://a/ws');
assert.equal(relayUrlFromHost('http://a:8080'), 'ws://a:8080');
assert.equal(relayUrlFromHost('https://a:8080'), 'wss://a:8080');
assert.equal(relayUrlFromHost('a:1/with/path'), 'ws://a:1/with/path');
assert.equal(DEFAULT_RELAY_HOST, '127.0.0.1:8080');
assert.ok(CONNECT_TIMEOUT_MS >= 3000, 'handshake budget sane');

// ---- relayHostFromSearch (?server= wins, persists via caller) ----
assert.deepEqual(relayHostFromSearch('', 'h'), { host: 'h', fromUrl: false });
assert.deepEqual(relayHostFromSearch('?server=192.168.1.9:8080', 'h'), { host: '192.168.1.9:8080', fromUrl: true });
assert.deepEqual(relayHostFromSearch('?server=ws://a:1/ws', 'h'), { host: 'ws://a:1/ws', fromUrl: true });
assert.deepEqual(relayHostFromSearch('?server=http://a:8080', 'h'), { host: 'a:8080', fromUrl: true });
assert.deepEqual(relayHostFromSearch('?server=http://a:8080/ws', 'h'), { host: 'a:8080', fromUrl: true });

// ---- REMOVED (owner order, item 5): fixtureIdToEngineCh sim-topology
// asserts deleted with the function (live.ts). Live meter lookup is
// topology-driven via bridgeStrip now.

// ---- linearToDb ----
assert.equal(linearToDb(0), -90);
assert.equal(linearToDb(-1), -90);
assert.equal(linearToDb(1), 0);
assert.ok(Math.abs(linearToDb(0.5) - -6.0206) < 0.01, 'half amplitude');
assert.equal(linearToDb(1e-10), -90, 'clamped');

// ---- auto-rejoin flag (no localStorage in node: defaults off) ----
assert.equal(loadRelayAuto(), false);

console.log('live.test.ts: ok');

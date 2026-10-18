// S78 Connection view-model tests (against design-logic.js.txt connectVals).
// Run: node --experimental-strip-types apps/web/src/ui/screens/connection/connectionVm.test.ts

import { strict as assert } from 'node:assert';
import {
  connectDevice,
  disconnect,
  goOffline,
  headerView,
  initialWorld,
  scanDevices,
  type World,
} from '../../state/uiLogic.ts';
import { buildConnectionVm } from './connectionVm.ts';

function dotOf(w: World): string {
  return headerView(w).connDot;
}

const w0 = initialWorld();
assert.equal(w0.state.connected, 'foh');

// Default fixture state: connected to FOH.
{
  const vm = buildConnectionVm(w0.state);
  assert.equal(vm.title, 'Connected to FOH Console');
  assert.equal(vm.sub, '192.168.1.20 · 48 kHz · latency 1.2 ms');
  assert.equal(vm.dot, '#5fe060');
  assert.equal(vm.dot, dotOf(w0));
  assert.equal(vm.halo, 'rgba(95,224,96,.18)');
  assert.equal(vm.cardBorder, '#2c4a2e');
  assert.equal(vm.isConnected, true);
  assert.equal(vm.lastScan, 'Scanned at start-up');
  assert.equal(vm.devices.length, 3);
  const [foh, mon, sb] = vm.devices;
  assert.equal(foh.name, 'FOH Console');
  assert.equal(foh.sub, '[Console model] · 192.168.1.20');
  assert.equal(foh.status, 'Connected');
  assert.equal(foh.statusColor, '#7be27b');
  assert.equal(foh.border, '#2c4a2e');
  assert.equal(foh.btnLabel, 'Connected');
  assert.equal(foh.btnBg, '#1a2a1b');
  assert.equal(mon.status, 'Available');
  assert.equal(mon.btnLabel, 'Connect');
  assert.equal(mon.btnBg, '#5cdb5c');
  assert.equal(mon.btnText, '#10240f');
  assert.equal(mon.canConnect, true);
  assert.equal(sb.status, 'I/O · in use');
  assert.equal(sb.statusColor, 'var(--u4-text-7)');
  assert.equal(sb.canConnect, false);
}

// Scan updates the last-scan text.
{
  const w = scanDevices(w0, '21:07');
  assert.equal(buildConnectionVm(w.state).lastScan, 'Scanned at 21:07');
}

// Connect switches the connected card + status card.
{
  const w = connectDevice(w0, 'mon');
  const vm = buildConnectionVm(w.state);
  assert.equal(vm.title, 'Connected to Monitor Console');
  assert.equal(vm.sub, '192.168.1.21 · 48 kHz · latency 1.2 ms');
  assert.equal(vm.devices[0].status, 'Available');
  assert.equal(vm.devices[1].status, 'Connected');
  assert.equal(vm.dot, dotOf(w));
}

// Disconnect clears the card.
{
  const w = disconnect(w0);
  const vm = buildConnectionVm(w.state);
  assert.equal(vm.title, 'Not connected');
  assert.equal(vm.sub, 'Pick a console below');
  assert.equal(vm.dot, '#ff4a3d');
  assert.equal(vm.halo, 'rgba(255,74,61,.18)');
  assert.equal(vm.cardBorder, '#5a2420');
  assert.equal(vm.isConnected, false);
  assert.equal(vm.dot, dotOf(w));
}

// Offline card state.
{
  const w = goOffline(w0);
  const vm = buildConnectionVm(w.state);
  assert.equal(vm.title, 'Working offline');
  assert.equal(vm.sub, 'Changes sync when you connect');
  assert.equal(vm.dot, '#f0b44a');
  assert.equal(vm.halo, 'rgba(240,180,74,.18)');
  assert.equal(vm.cardBorder, '#5a4420');
  assert.equal(vm.isConnected, false);
}

// Live relay state owns the status card when a link is active.
{
  const base = { phase: 'connecting' as const, message: 'Connecting to h …', host: 'h', latencyMs: null, strips: 0 };
  const busy = buildConnectionVm(w0.state, base);
  assert.equal(busy.title, 'Connecting to relay …');
  assert.equal(busy.dot, '#f0b44a');
  assert.equal(busy.isConnected, false);

  const on = buildConnectionVm(w0.state, { ...base, phase: 'online', message: 'Live · sim · 77 strips', latencyMs: 12, strips: 77 });
  assert.equal(on.title, 'Live · relay link');
  assert.equal(on.sub, 'Live · sim · 77 strips · h');
  assert.equal(on.dot, '#5fe060');

  const err = buildConnectionVm(w0.state, { ...base, phase: 'error', message: 'No response from relay at h' });
  assert.equal(err.title, 'Relay connection failed');
  assert.equal(err.sub, 'No response from relay at h');
  assert.equal(err.dot, '#ff4a3d');
}

console.log('connectionVm.test.ts: ok');

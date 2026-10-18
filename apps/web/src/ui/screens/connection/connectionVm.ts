// S78 Connection screen: pure view-model builder.
//
// Exact port of the `connectVals` branch of the design's renderVals
// (docs/design/src/design-logic.js.txt). The status dot formula is shared
// with headerView (uiLogic.ts): online green, offline amber, else red.
// Run: node --experimental-strip-types apps/web/src/ui/screens/connection/connectionVm.test.ts

import { devices } from '../../fixtures/designData.ts';
import { headerView, type UiState, type World } from '../../state/uiLogic.ts';

export const CONN_ICONS: Record<string, string> = {
  console: 'M3 5h18v14H3zM7 9v6M11 9v6M15 9v6M19 9v6',
  io: 'M4 7h16v10H4zM7 12h.01M10 12h.01M13 12h.01M16 12h.01',
};

export interface DeviceVm {
  key: string;
  name: string;
  sub: string;
  icon: string;
  status: string;
  statusColor: string;
  border: string;
  canConnect: boolean;
  btnLabel: string;
  btnBg: string;
  btnText: string;
  btnBorder: string;
}

export interface ConnectionVm {
  dot: string;
  halo: string;
  cardBorder: string;
  title: string;
  sub: string;
  isConnected: boolean;
  lastScan: string;
  devices: DeviceVm[];
}

export function buildConnectionVm(state: UiState): ConnectionVm {
  const devs = devices();
  const cur = devs.find(d => d.key === state.connected);
  const offline = state.connected === 'offline';
  // headerView owns the dot contract; reuse it instead of duplicating.
  const dot = headerView({ state, chans: {}, fx: [] } as World).connDot;
  return {
    dot,
    halo: cur ? 'rgba(95,224,96,.18)' : offline ? 'rgba(240,180,74,.18)' : 'rgba(255,74,61,.18)',
    cardBorder: cur ? '#2c4a2e' : offline ? '#5a4420' : '#5a2420',
    title: cur ? 'Connected to ' + cur.name : offline ? 'Working offline' : 'Not connected',
    sub: cur
      ? cur.ip + ' · 48 kHz · latency 1.2 ms'
      : offline
        ? 'Changes sync when you connect'
        : 'Pick a console below',
    isConnected: !!cur,
    lastScan: state.lastScan,
    devices: devs.map(d => {
      const on = d.key === state.connected;
      return {
        key: d.key,
        name: d.name,
        sub: d.model + ' · ' + d.ip,
        icon: CONN_ICONS[d.kind],
        status: on ? 'Connected' : d.kind === 'io' ? 'I/O · in use' : 'Available',
        statusColor: on ? '#7be27b' : d.kind === 'io' ? '#8792a4' : '#c9d1dc',
        border: on ? '#2c4a2e' : '#1f2836',
        canConnect: d.kind === 'console',
        btnLabel: on ? 'Connected' : 'Connect',
        btnBg: on ? '#1a2a1b' : '#5cdb5c',
        btnText: on ? '#7be27b' : '#10240f',
        btnBorder: on ? '#2c4a2e' : '#5cdb5c',
      };
    }),
  };
}

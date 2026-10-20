// Connection view model. Live hardware status comes only from the shared
// session selector; discovered rows are not evidence of an active session.
// The no-live branch remains available for explicit design fixtures.

import { sessionStatus } from '../../state/sessionStatus.ts';
import type { LinkView } from '../../world/types.ts';
import { devices } from '../../fixtures/designData.ts';
import { headerView, type UiState, type World } from '../../state/uiLogic.ts';
import type { LivePhase } from '../../state/live.ts';

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
  /** Scan button state: busy shows a disabled "Scanning…" label. */
  scanLabel: string;
  scanBusy: boolean;
  /** Loading row under the device list while a sweep is in flight. */
  scanHint: string | null;
  /** Retry hint when the last scan never reported back (else null). */
  scanError: string | null;
  devices: DeviceVm[];
  /** Hint shown under the device list when it is honestly empty (null =
   *  rows present). The relay link is automatic and never shown here, so
   *  there is no fix button: desks either answer Scan or they don't. */
  emptyText: string | null;
  /** Always false now (the relay link is automatic — no manual host UI).
   *  Kept so layout bindings and existing tests keep their shape. */
  showFix: boolean;
  /** Manual add-by-IP card: visible while a relay link can probe (online,
   *  or errored — the probe still needs the link, so the card explains
   *  itself while down). Add probes AND connects: the verified desk joins
   *  the device list above with `via: manual` and the link flips to
   *  Connected without a second tap. Never any 127.0.0.1/relay-link UI
   *  here (that stays deleted). */
  showManualDesk: boolean;
}

/** Live relay state the status card prefers over the fixture picker. */
export interface LiveStatus {
  phase: LivePhase;
  capsReady?: boolean;
  link?: LinkView | null;
  message: string;
  host: string;
  latencyMs: number | null;
  strips: number;
  /** Engine devices (scan results) when online; undefined = never scanned. */
  devices?: Array<{ id: string; name: string; model: string; ip: string; kind: string; state: string; fw?: string | null; via?: string | null }>;
  /** Engine link console name (the desk we are actually on). */
  linkName?: string | null;
  /** Engine scan epoch (0/absent = never scanned this session). */
  scannedAt?: number;
  /** Client-side scan-busy flag (no engine scan-started event exists). */
  scanning?: boolean;
  /** Honest scan failure hint (pressed with no link, or sweep timed out). */
  scanError?: string | null;
  /** Engine driver behind the relay (informational only — the device
   *  list never carries sim rows). */
  driver?: string;
  /** One-tap relay fix for this device (`<page-host>:<relay-port>`), or
   *  null when the page itself is local (localhost default stays). */
  suggestion?: string | null;
}

function liveDeviceVms(
  devs: NonNullable<LiveStatus['devices']>,
  linkIp: string | null | undefined,
): DeviceVm[] {
  // Only real desks ever reach this list (the engine never emits sim
  // seeds; the filter below drops any legacy/foreign via-less row).
  // Cross-subnet rows carry their source: the relay's linked desk
  // (unicast verified) or a manual add-by-IP probe — broadcast needs
  // no badge.
  return devs
    .filter(d => ['broadcast', 'linked', 'manual'].includes(d.via ?? '') && !!d.ip.trim())
    .map(d => {
      const on = !!linkIp && d.ip === linkIp;
      const via = !on && (d.via === 'linked' || d.via === 'manual')
        ? d.via === 'linked'
          ? ' · linked desk'
          : ' · added by IP'
        : '';
      return {
        key: d.id,
        name: d.name.trim() || d.model || 'Mixer',
        sub: d.model + ' · ' + d.ip + (d.fw ? ' · FW ' + d.fw : '') + via,
        icon: CONN_ICONS[d.kind] ?? CONN_ICONS['console']!,
        status: on ? 'Connected' : d.state === 'connected' ? 'Reachable' : d.state,
        statusColor: on ? '#7be27b' : '#c9d1dc',
        border: on ? '#2c4a2e' : '#1f2836',
        // The connected row states it in text; the status card owns Disconnect.
        // A "Connected" button next to a "Connected" label is dead chrome.
        canConnect: d.kind === 'console' && !on,
        btnLabel: on ? 'Connected' : 'Connect',
        btnBg: on ? '#1a2a1b' : '#5cdb5c',
        btnText: on ? '#7be27b' : '#10240f',
        btnBorder: on ? '#2c4a2e' : '#5cdb5c',
      };
    });
}

export function buildConnectionVm(state: UiState, live?: LiveStatus): ConnectionVm {
  const devs = devices();
  const cur = devs.find(d => d.key === state.connected);
  const offline = state.connected === 'offline';
  // headerView owns the dot contract; reuse it instead of duplicating.
  const dot = headerView({ state, chans: {}, fx: [] } as World).connDot;
  // A live relay link owns the socket but is NEVER shown: the frontend<>
  // backend link auto-connects at boot (Tauri IPC in-process, WS relay
  // default) and the connection screen is only about DESKS. Scan results
  // are the ONLY device source here: real discovered desks, an honest
  // empty state, or a press-Scan hint — fixtures never leak in, and no
  // "Live · relay link" / host / 127.0.0.1 chrome ever renders.
  if (live) {
    const status = sessionStatus(live.phase, live.capsReady === true, live.driver ?? '', live.link ?? null, offline);
    const scanned = (live.scannedAt ?? 0) > 0;
    const scanBusy = live.scanning === true && !offline;
    const found = status.ready ? liveDeviceVms(live.devices ?? [], status.connected ? live.link?.ip : null) : [];
    return {
      dot: status.color, halo: 'transparent', cardBorder: status.color,
      title: status.label, sub: status.detail, isConnected: status.connected,
      lastScan: scanBusy ? 'Searching this network…' : scanned
        ? 'Last scan ' + new Date(live.scannedAt! * 1000).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }) : 'Ready when you are',
      scanLabel: scanBusy ? 'Searching…' : scanned ? 'Scan again' : 'Find mixers',
      scanBusy, scanHint: scanBusy ? 'Waiting for mixers to reply…' : null,
      scanError: live.scanError ?? null, devices: found,
      emptyText: found.length || scanBusy || live.scanError ? null : scanned
        ? 'No mixers found. Check the mixer’s power and Wi-Fi network, or enter its IP address.'
        : 'Your mixer will appear here after a scan.',
      showFix: !status.ready && !offline, showManualDesk: !offline,
    };
  }
  // No relay link was ever picked and no fixture console is selected: say
  // so instead of inventing desks (boot / disconnected state).
  if (!cur && !offline) {
    return {
      dot,
      halo: 'rgba(255,74,61,.18)',
      cardBorder: '#5a2420',
      title: 'Not connected',
      sub: 'Scan this network or connect by IP address',
      isConnected: false,
      lastScan: state.lastScan,
      scanLabel: 'Scan devices on this network',
      scanBusy: false,
      scanHint: null,
      scanError: null,
      devices: [],
      emptyText:
        'Scan this network to find available mixers.',
      showFix: false,
      showManualDesk: false,
    };
  }
  // The design fixture itself (`?fixture=1` dev route only): fixture
  // consoles render here, never mixed into scan results. Explicit
  // offline mode is the sim (full local demo desk, zero network), so its
  // device list stays honestly empty — seed rows are never shown as
  // hardware in the live app.
  if (offline) {
    return {
      dot,
      halo: 'rgba(240,180,74,.18)',
      cardBorder: '#5a4420',
      title: 'Working offline',
      sub: 'Local preview only. Go online to choose a mixer.',
      isConnected: false,
      lastScan: state.lastScan,
      scanLabel: 'Scan devices on this network',
      scanBusy: false,
      scanHint: null,
      scanError: null,
      devices: [],
      emptyText:
        'Go online to enable network discovery and connect by IP address. Offline changes are not sent to a mixer.',
      showFix: false,
      showManualDesk: false,
    };
  }
  return {
    dot,
    halo: 'rgba(95,224,96,.18)',
    cardBorder: '#2c4a2e',
    title: 'Connected to ' + cur!.name,
    sub: cur!.ip + ' · 48 kHz · latency 1.2 ms',
    isConnected: true,
    lastScan: state.lastScan,
    scanLabel: 'Scan devices on this network',
    scanBusy: false,
    scanHint: null,
    scanError: null,
    devices: deviceVms(devs, state.connected),
    emptyText: null,
    showFix: false,
    showManualDesk: false,
  };
}

function deviceVms(
  devs: ReturnType<typeof devices>,
  connected: string | null,
): DeviceVm[] {
  return devs.map(d => {
    const on = d.key === connected;
    return {
      key: d.key,
      name: d.name.trim() || d.model || 'Mixer',
      sub: d.model + ' · ' + d.ip,
      icon: CONN_ICONS[d.kind],
      status: on ? 'Connected' : d.kind === 'io' ? 'I/O · in use' : 'Available',
      statusColor: on ? '#7be27b' : d.kind === 'io' ? 'var(--u4-text-7)' : 'var(--u4-text-4)',
      border: on ? '#2c4a2e' : 'var(--u4-line-panel)',
      // The connected row states it in text; the status card owns Disconnect.
      // A "Connected" button next to a "Connected" label is dead chrome.
      canConnect: d.kind === 'console' && !on,
      btnLabel: on ? 'Connected' : 'Connect',
      btnBg: on ? '#1a2a1b' : '#5cdb5c',
      btnText: on ? '#7be27b' : '#10240f',
      btnBorder: on ? '#2c4a2e' : '#5cdb5c',
    };
  });
}

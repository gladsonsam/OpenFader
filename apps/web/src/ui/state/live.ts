// Live relay helpers: pure, node-testable URL/host rules + the fixture id
// to engine strip mapping + meter conversion for the live binding.
// The reactive singleton lives in live.svelte.ts (runes need the Svelte
// compiler, so plain node can't import it; this file stays rune-free).
//
// Run: node --experimental-strip-types apps/web/src/ui/state/live.test.ts

export type LivePhase = 'idle' | 'connecting' | 'online' | 'error';

export const RELAY_LS_KEY = 'openmix.relay-host';
/** Set when the user connects; cleared on manual disconnect (auto-rejoin). */
export const RELAY_AUTO_KEY = 'openmix.relay-auto';
export const DEFAULT_RELAY_HOST = '127.0.0.1:8080';
/** Handshake budget: caps must arrive this soon after connect. Raised past
 * the old 6 s: post-switch first paint is local-speed, but a lossy-venue
 * connect burst (300 ms per query, several queries) can legitimately take
 * seconds before caps lands — expiring here while the engine is still
 * working would flip the UI to error and invite a retry double-load. */
export const CONNECT_TIMEOUT_MS = 15000;
/** Scan busy budget: the engine sends no scan-started event (devices only
 *  lands when the sweep finishes), so the UI clears its busy state on the
 *  next devices frame or after this long, whichever comes first. */
export const SCAN_TIMEOUT_MS = 10000;

/** Shared offline intent for both the UI and transport lifecycle. */
const OFFLINE_LS_KEY = 'openmix-offline';
export function loadOfflineMode(): boolean {
  try { return storage()?.getItem(OFFLINE_LS_KEY) === '1'; } catch { return false; }
}
export function saveOfflineMode(value: boolean): void {
  try {
    if (value) storage()?.setItem(OFFLINE_LS_KEY, '1');
    else storage()?.removeItem(OFFLINE_LS_KEY);
  } catch { /* Storage can be unavailable in private WebViews. */ }
}

function storage(): Storage | null {
  try {
    return typeof localStorage === 'undefined' ? null : localStorage;
  } catch {
    return null;
  }
}

/** Persisted relay host (bare `host:port`). */
export function loadRelayHost(): string {
  try {
    const raw = storage()?.getItem(RELAY_LS_KEY)?.trim();
    if (raw) return raw;
  } catch {
    /* ignore */
  }
  return DEFAULT_RELAY_HOST;
}

/** Whether the last session left the link up (auto-rejoin on boot). */
export function loadRelayAuto(): boolean {
  try {
    return storage()?.getItem(RELAY_AUTO_KEY) === '1';
  } catch {
    return false;
  }
}

/** Persist the auto-rejoin intent (set on connect, cleared on fail/exit). */
export function saveRelayAuto(v: boolean): void {
  try {
    if (v) storage()?.setItem(RELAY_AUTO_KEY, '1');
    else storage()?.removeItem(RELAY_AUTO_KEY);
  } catch {
    /* ignore */
  }
}

export function saveRelayHost(host: string): void {
  try {
    storage()?.setItem(RELAY_LS_KEY, host.trim());
  } catch {
    /* ignore */
  }
}

/**
 * Normalise a relay host field into a WS URL. Bare `host:port` (or a LAN
 * IP typed on a phone) becomes `ws://host:port/ws`; full ws(s):// and
 * http(s):// values convert like `?server=` (see transport.ts).
 */
export function relayUrlFromHost(host: string): string {
  const s = host.trim();
  if (!s) return `ws://${DEFAULT_RELAY_HOST}/ws`;
  if (/^wss?:\/\//i.test(s)) return s;
  if (/^https:\/\//i.test(s)) return `wss://${s.slice(8)}`;
  if (/^http:\/\//i.test(s)) return `ws://${s.slice(7)}`;
  return /[/ ?#]/.test(s) ? `ws://${s}` : `ws://${s}/ws`;
}

/** `?server=` from the URL wins over the stored host (caller persists it). */
export function relayHostFromSearch(search: string, fallback: string): { host: string; fromUrl: boolean } {
  const q = new URLSearchParams(search.startsWith('?') ? search : '?' + search);
  const s = (q.get('server') || '').trim();
  if (!s) return { host: fallback, fromUrl: false };
  if (/^wss?:\/\//i.test(s)) return { host: s, fromUrl: true };
  const noScheme = s.replace(/^https?:\/\//i, '');
  return { host: noScheme.replace(/\/ws\/?$/, '').replace(/\/$/, ''), fromUrl: true };
}

/** Host part of a `host:port` relay value (brackets stripped). */
export function relayHostPart(host: string): string {
  const s = host.trim();
  const m = s.match(/^\[([^\]]+)\](?::\d+)?$/);
  if (m) return m[1]!;
  const idx = s.lastIndexOf(':');
  if (idx > 0 && /^\d+$/.test(s.slice(idx + 1))) return s.slice(0, idx);
  return s;
}

/** Port part of a `host:port` relay value (default relay port when bare). */
export function relayPortPart(host: string): string {
  const s = host.trim();
  const m = s.match(/^\[([^\]]+)\](?::(\d+))?$/);
  if (m) return m[2] ?? relayPortPart(DEFAULT_RELAY_HOST);
  const idx = s.lastIndexOf(':');
  if (idx > 0 && /^\d+$/.test(s.slice(idx + 1))) return s.slice(idx + 1);
  return relayPortPart(DEFAULT_RELAY_HOST);
}

/** Loopback relay hosts: 127/8, localhost, ::1 — this device itself. */
export function isLoopbackHost(host: string): boolean {
  const h = host.trim().toLowerCase().replace(/^\[|\]$/g, '');
  return (
    h === 'localhost' ||
    h === '::1' ||
    h === '0.0.0.0' ||
    /^127(\.\d{1,3}){3}$/.test(h)
  );
}

/**
 * One-tap fix when the relay is unreachable from THIS device: the relay
 * usually runs on the machine serving this page, so suggest
 * `<page-host>:<relay-port>`. Returns null when the page itself is local
 * (localhost default stays) or when there is nothing better to suggest —
 * and NEVER suggests 127.0.0.1 to a remote client.
 */
export function pageHostSuggestion(configuredHost: string, pageHostname: string): string | null {
  const page = pageHostname.trim().replace(/^\[|\]$/g, '');
  if (!page || isLoopbackHost(page)) return null;
  const cur = relayHostPart(configuredHost);
  const suggestion = `${page}:${relayPortPart(configuredHost)}`;
  if (cur.toLowerCase() === page.toLowerCase()) return null;
  if (suggestion === configuredHost.trim()) return null;
  return suggestion;
}

/** Recent manual-desk hosts (add-by-IP): most recent first, max 5. */
const RECENT_DESKS_KEY = 'openmix.recent-desks';
const MAX_RECENT_DESKS = 5;

export function loadRecentDesks(): string[] {
  try {
    const raw = storage()?.getItem(RECENT_DESKS_KEY);
    if (!raw) return [];
    const arr: unknown = JSON.parse(raw);
    if (!Array.isArray(arr)) return [];
    return arr.filter((v): v is string => typeof v === 'string' && v.trim().length > 0).slice(0, MAX_RECENT_DESKS);
  } catch {
    return [];
  }
}

/** Remember a manually probed desk host (dedupe, most-recent-first). */
export function saveRecentDesk(host: string): string[] {
  const h = host.trim();
  const next = [h, ...loadRecentDesks().filter(d => d.toLowerCase() !== h.toLowerCase())].slice(
    0,
    MAX_RECENT_DESKS,
  );
  try {
    storage()?.setItem(RECENT_DESKS_KEY, JSON.stringify(next));
  } catch {
    /* ignore */
  }
  return next;
}

/**
 * REMOVED (owner order, item 5): the old sim-topology fixture id -> engine
 * strip index map (`in1` -> 0 … `mainlr` -> 72). It fed every live meter
 * the wrong strip on any non-sim desk (XR18: `mainlr` -> 72 is out of
 * range, buses land on other families' blobs) and the null fallback
 * animated the fixture sim over live pages (phantom meters). Live meter
 * lookup is topology-driven now (`bridgeStrip` in engineBridge.ts).
 */

/** Linear 0..1 meter tap -> dB for the canvas meters (-90 = silence). */
export function linearToDb(v: number): number {
  if (!(v > 0)) return -90;
  const db = 20 * Math.log10(v);
  return db < -90 ? -90 : db;
}

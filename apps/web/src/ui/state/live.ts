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
/** Handshake budget: caps must arrive this soon after connect. */
export const CONNECT_TIMEOUT_MS = 6000;

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

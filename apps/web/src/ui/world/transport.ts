// W2a transport: the tiny socket seam under EngineWorld.
//
// `Transport` carries the engine Cmd/Event JSON (text) plus WS BINARY meter
// frames; WS now, Tauri IPC later (ME2). `WsTransport` is the browser
// implementation (native WebSocket, binaryType arraybuffer, auto-reconnect
// with backoff, `hello {binary_meters:true}` on every connect).
// `ReplayTransport` feeds scripted golden frames for node tests.
// `TauriTransport` is a documented stub: it satisfies the interface so the
// World can be constructed, but every operation throws until ME2 wires the
// Tauri IPC adapter (which speaks the same Cmd/Event JSON).

import { cmdHello } from './protocol.ts';

export type TransportMessage = string | ArrayBuffer;
export type Unsub = () => void;

/** Minimal socket shape WsTransport drives (native WebSocket compatible). */
export interface WsSocket {
  binaryType: string;
  readonly readyState: number;
  send(data: string): void;
  close(): void;
  onopen: ((ev: unknown) => void) | null;
  onclose: ((ev: unknown) => void) | null;
  onerror: ((ev: unknown) => void) | null;
  onmessage: ((ev: { data: unknown }) => void) | null;
}

export interface Transport {
  readonly url: string;
  /** Open the socket (idempotent; reconnects automatically on drop for WS). */
  connect(): void;
  /** Send one engine command (JSON string from protocol.ts builders). */
  send(json: string): void;
  onMessage(cb: (msg: TransportMessage) => void): Unsub;
  onStatus(cb: (online: boolean) => void): Unsub;
  close(): void;
}

/**
 * Pure URL rule (mirrors lib/backend.ts wsUrlFromQuery — same behaviour, no
 * import: backend.ts is read-only and pulls the Tauri shell).
 * `?server=host:port` => ws(s)://host:port/ws; full ws(s):// URLs and paths
 * pass through; else same-origin /ws.
 */
export function wsUrlFromQueryString(
  search: string,
  protocol: string,
  host: string,
  hostname: string,
): string {
  const q = new URLSearchParams(search.startsWith('?') ? search : '?' + search);
  const s = (q.get('server') || '').trim();
  if (/^wss?:\/\//i.test(s)) return s;
  if (/^https:\/\//i.test(s)) return `wss://${s.slice(8)}`;
  if (/^http:\/\//i.test(s)) return `ws://${s.slice(7)}`;
  if (s) {
    const scheme = protocol === 'https:' ? 'wss' : 'ws';
    return /[/ ?#]/.test(s) ? `${scheme}://${s}` : `${scheme}://${s}/ws`;
  }
  const pageProto = protocol === 'https:' ? 'wss' : 'ws';
  if (!hostname) return 'ws://127.0.0.1:8080';
  return `${pageProto}://${host}/ws`;
}

/** Browser helper: derive the relay URL from window.location. */
export function wsUrlFromLocation(): string {
  const loc = (globalThis as { location?: { search: string; protocol: string; host: string; hostname: string } }).location;
  if (!loc) return 'ws://127.0.0.1:8080/ws';
  return wsUrlFromQueryString(loc.search, loc.protocol, loc.host, loc.hostname);
}

export interface WsTransportOptions {
  /** Socket factory (default: native WebSocket; inject a fake in tests). */
  createSocket?: (url: string) => WsSocket;
  /** Reconnect delays in ms (default backoff 1s/2s/5s/10s, capped). */
  backoffMs?: number[];
  /** Negotiate binary meters + taps on connect (default true). */
  binaryMeters?: boolean;
  wantTaps?: boolean;
  /** Timer injection for tests (default setTimeout/clearTimeout). */
  setTimer?: (fn: () => void, ms: number) => unknown;
  clearTimer?: (t: unknown) => void;
}

const DEFAULT_BACKOFF = [1000, 2000, 5000, 10000];

export class WsTransport implements Transport {
  readonly url: string;
  private readonly opts: WsTransportOptions;
  private socket: WsSocket | null = null;
  private readonly queue: string[] = [];
  private readonly msgCbs = new Set<(msg: TransportMessage) => void>();
  private readonly statusCbs = new Set<(online: boolean) => void>();
  private online = false;
  private wantOpen = false;
  private failures = 0;
  private retryTimer: unknown = null;
  private manuallyClosed = false;

  constructor(url?: string, opts: WsTransportOptions = {}) {
    this.url = url ?? wsUrlFromLocation();
    this.opts = opts;
  }

  connect(): void {
    this.wantOpen = true;
    this.manuallyClosed = false;
    if (!this.socket) this.open();
  }

  private setOnline(v: boolean): void {
    if (this.online === v) return;
    this.online = v;
    this.statusCbs.forEach((cb) => {
      try {
        cb(v);
      } catch {
        /* ignore */
      }
    });
  }

  private clearRetry(): void {
    if (this.retryTimer !== null) {
      (this.opts.clearTimer ?? clearTimeout)(this.retryTimer);
      this.retryTimer = null;
    }
  }

  private scheduleRetry(): void {
    if (!this.wantOpen || this.manuallyClosed || this.retryTimer !== null) return;
    const backoff = this.opts.backoffMs ?? DEFAULT_BACKOFF;
    const ms = backoff[Math.min(this.failures, backoff.length - 1)] ?? 10000;
    this.retryTimer = (this.opts.setTimer ?? setTimeout)(() => {
      this.retryTimer = null;
      this.open();
    }, ms);
  }

  private open(): void {
    this.clearRetry();
    const create =
      this.opts.createSocket ??
      ((url: string): WsSocket => {
        const Ws = (globalThis as { WebSocket?: new (url: string) => WsSocket }).WebSocket;
        if (!Ws) throw new Error('no WebSocket available for ' + url);
        return new Ws(url);
      });
    let ws: WsSocket;
    try {
      ws = create(this.url);
    } catch {
      this.failures++;
      this.scheduleRetry();
      return;
    }
    this.socket = ws;
    try {
      ws.binaryType = 'arraybuffer';
    } catch {
      /* fake sockets may ignore */
    }
    ws.onopen = () => {
      this.failures = 0;
      this.setOnline(true);
      for (const m of this.queue.splice(0)) {
        try {
          ws.send(m);
        } catch {
          this.queue.unshift(m);
          break;
        }
      }
      // Negotiate binary meters (+taps) on EVERY connect (incl. reconnects):
      // the preference is per-client on the engine and resets on drop.
      try {
        ws.send(cmdHello(this.opts.binaryMeters ?? true, this.opts.wantTaps ?? true ? true : undefined));
      } catch {
        /* queued implicitly on next failure */
      }
    };
    ws.onmessage = (ev: { data: unknown }) => {
      const d = ev.data;
      if (typeof d === 'string') this.emit(d);
      else if (d instanceof ArrayBuffer) this.emit(d);
      else if (typeof ArrayBuffer !== 'undefined' && ArrayBuffer.isView(d)) {
        const v = d as Uint8Array;
        this.emit(v.buffer.slice(v.byteOffset, v.byteOffset + v.byteLength));
      }
    };
    const down = (): void => {
      if (this.socket === ws) this.socket = null;
      this.setOnline(false);
      this.failures++;
      this.scheduleRetry();
    };
    ws.onclose = down;
    ws.onerror = () => {
      /* onclose reports; some fakes only fire onerror */
    };
  }

  private emit(msg: TransportMessage): void {
    this.msgCbs.forEach((cb) => {
      try {
        cb(msg);
      } catch {
        /* ignore */
      }
    });
  }

  send(json: string): void {
    const open = this.socket && (this.socket.readyState as number) === 1;
    if (open && this.socket) {
      try {
        this.socket.send(json);
        return;
      } catch {
        this.socket = null;
        this.setOnline(false);
      }
    }
    this.queue.push(json);
    if (this.wantOpen && !this.manuallyClosed && !this.socket) this.open();
  }

  onMessage(cb: (msg: TransportMessage) => void): Unsub {
    this.msgCbs.add(cb);
    return () => {
      this.msgCbs.delete(cb);
    };
  }

  onStatus(cb: (online: boolean) => void): Unsub {
    this.statusCbs.add(cb);
    try {
      cb(this.online);
    } catch {
      /* ignore */
    }
    return () => {
      this.statusCbs.delete(cb);
    };
  }

  close(): void {
    this.manuallyClosed = true;
    this.wantOpen = false;
    this.clearRetry();
    try {
      this.socket?.close();
    } catch {
      /* ignore */
    }
    this.socket = null;
    this.setOnline(false);
  }
}

/**
 * Scripted transport for node tests (and the debug screen's fixture mode):
 * records every sent command; `feedText`/`feedBinary` deliver engine frames;
 * `setOnline` flips the status flag. `connect()` is a no-op (already open).
 */
export class ReplayTransport implements Transport {
  readonly url = 'replay://test';
  readonly sent: string[] = [];
  private readonly msgCbs = new Set<(msg: TransportMessage) => void>();
  private readonly statusCbs = new Set<(online: boolean) => void>();
  private online = true;

  connect(): void {
    this.emitStatus();
  }

  send(json: string): void {
    this.sent.push(json);
  }

  /** Last sent command parsed as JSON (null when empty/unparseable). */
  lastSent(): Record<string, unknown> | null {
    if (!this.sent.length) return null;
    try {
      const v: unknown = JSON.parse(this.sent[this.sent.length - 1]!);
      return typeof v === 'object' && v !== null ? (v as Record<string, unknown>) : null;
    } catch {
      return null;
    }
  }

  feedText(json: string): void {
    this.msgCbs.forEach((cb) => {
      try {
        cb(json);
      } catch {
        /* ignore */
      }
    });
  }

  feedBinary(buf: ArrayBuffer | Uint8Array): void {
    const msg: ArrayBuffer = buf instanceof Uint8Array ? buf.buffer.slice(buf.byteOffset, buf.byteOffset + buf.byteLength) : buf;
    this.msgCbs.forEach((cb) => {
      try {
        cb(msg);
      } catch {
        /* ignore */
      }
    });
  }

  setOnline(v: boolean): void {
    this.online = v;
    this.emitStatus();
  }

  private emitStatus(): void {
    this.statusCbs.forEach((cb) => {
      try {
        cb(this.online);
      } catch {
        /* ignore */
      }
    });
  }

  onMessage(cb: (msg: TransportMessage) => void): Unsub {
    this.msgCbs.add(cb);
    return () => {
      this.msgCbs.delete(cb);
    };
  }

  onStatus(cb: (online: boolean) => void): Unsub {
    this.statusCbs.add(cb);
    try {
      cb(this.online);
    } catch {
      /* ignore */
    }
    return () => {
      this.statusCbs.delete(cb);
    };
  }

  close(): void {}
}

/**
 * Tauri IPC stub (ME2 owns the real adapter). Constructing is fine; every
 * operation throws a descriptive error so a miswired build fails loudly
 * instead of silently serving no data. The ME2 adapter will speak the same
 * Cmd/Event JSON over `invoke('engine_cmd')` + a forward channel.
 */
export class TauriTransport implements Transport {
  readonly url = 'tauri://engine';
  connect(): void {
    throw new Error('TauriTransport: Tauri IPC adapter lands in ME2 (not wired yet)');
  }
  send(_json: string): void {
    throw new Error('TauriTransport: Tauri IPC adapter lands in ME2 (not wired yet)');
  }
  onMessage(_cb: (msg: TransportMessage) => void): Unsub {
    throw new Error('TauriTransport: Tauri IPC adapter lands in ME2 (not wired yet)');
  }
  onStatus(_cb: (online: boolean) => void): Unsub {
    throw new Error('TauriTransport: Tauri IPC adapter lands in ME2 (not wired yet)');
  }
  close(): void {}
}

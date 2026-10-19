// W2a transport: the tiny socket seam under EngineWorld.
//
// `Transport` carries the engine Cmd/Event JSON (text) plus WS BINARY meter
// frames; `WsTransport` is the browser implementation (native WebSocket,
// binaryType arraybuffer, auto-reconnect with backoff, `hello
// {binary_meters:true}` on every connect). `ReplayTransport` feeds scripted
// golden frames for node tests. `TauriTransport` is the app-shell
// implementation: same Cmd/Event JSON over the embedded engine's IPC
// commands (`engine_attach`/`engine_cmd`/`engine_poll`), polled at 10 Hz,
// with `hello {binary_meters:false}` (IPC has no binary channel).

import { cmdHello } from './protocol.ts';

/** True when the page runs inside the Tauri shell (desktop or mobile):
 * the embedded engine is reachable over IPC and no relay is needed.
 * Tauri 2 exposes `__TAURI_INTERNALS__`; older shells exposed `__TAURI__`
 * (checked too so webviews on any shell version take the IPC path). */
export function isTauriRuntime(): boolean {
  try {
    const w = globalThis as {
      __TAURI_INTERNALS__?: unknown;
      __TAURI__?: unknown;
    };
    return w.__TAURI_INTERNALS__ !== undefined || w.__TAURI__ !== undefined;
  } catch {
    return false;
  }
}

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
 * Tauri IPC transport: speaks the same engine Cmd/Event JSON as
 * `WsTransport`, but against the engine embedded in the app process
 * (`engine_attach`/`engine_cmd`/`engine_poll`/`engine_detach` in
 * `src-tauri/src/lib.rs`) instead of a WS relay. No host, no port, no
 * LAN server: discovery/scan runs in-process on the phone's WiFi.
 *
 * Framing: `send(json)` invokes `engine_cmd` and feeds the returned Event
 * frames; a poll loop (`engine_poll`, default 100 ms) collects background
 * frames (meter ticker, param poller, discovery). Binary IPC is never
 * used — hello opts out of binary meters, so every frame arrives as the
 * same JSON text the relay would send (byte-identical command spellings,
 * text-only event path).
 *
 * The `invoke` function is injectable so node tests (and the debug
 * fixture) can drive the transport without a Tauri shell; by default it
 * lazy-imports `@tauri-apps/api/core` on first use, so merely importing
 * this module in a browser never touches the Tauri runtime.
 */
export interface TauriInvokeFn {
  (cmd: string, args?: Record<string, unknown>): Promise<unknown>;
}

export interface TauriTransportOptions {
  /** IPC function (default: lazy `@tauri-apps/api/core` invoke). */
  invoke?: TauriInvokeFn;
  /** Background poll interval in ms (default 100). */
  pollMs?: number;
  /** Timer injection for tests (default setInterval/clearInterval). */
  setTimer?: (fn: () => void, ms: number) => unknown;
  clearTimer?: (t: unknown) => void;
}

const TAURI_POLL_MS = 100;

export class TauriTransport implements Transport {
  readonly url = 'tauri://engine';
  private readonly invokeOpt: TauriInvokeFn | null;
  private readonly pollMs: number;
  private readonly setTimer: (fn: () => void, ms: number) => unknown;
  private readonly clearTimer: (t: unknown) => void;
  private readonly queue: string[] = [];
  private readonly msgCbs = new Set<(msg: TransportMessage) => void>();
  private readonly statusCbs = new Set<(online: boolean) => void>();
  private online = false;
  private attached = false;
  private closed = false;
  private polling = false;
  private timer: unknown = null;
  private attachPromise: Promise<void> | null = null;

  constructor(opts: TauriTransportOptions = {}) {
    this.invokeOpt = opts.invoke ?? null;
    this.pollMs = opts.pollMs ?? TAURI_POLL_MS;
    this.setTimer = opts.setTimer ?? ((fn, ms) => setInterval(fn, ms));
    this.clearTimer = opts.clearTimer ?? ((t) => clearInterval(t as ReturnType<typeof setInterval>));
  }

  private invoke(cmd: string, args?: Record<string, unknown>): Promise<unknown> {
    if (this.invokeOpt) return this.invokeOpt(cmd, args);
    return import('@tauri-apps/api/core').then((m) => m.invoke(cmd, args));
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

  private feed(frames: unknown): void {
    if (!Array.isArray(frames)) return;
    for (const f of frames) {
      if (typeof f !== 'string') continue;
      const msg: TransportMessage = f;
      this.msgCbs.forEach((cb) => {
        try {
          cb(msg);
        } catch {
          /* ignore */
        }
      });
    }
  }

  private ensureAttached(): Promise<void> {
    if (this.attached) return Promise.resolve();
    if (!this.attachPromise) {
      this.attachPromise = this.invoke('engine_attach', { label: 'ui' }).then(
        () => {
          if (this.closed) return;
          this.attached = true;
          // Same hello the WS path sends on every (re)connect, minus
          // binary meters (IPC has no binary channel): opts the client
          // into meter taps, so EngineWorld gets the full meter fan-out.
          const hello = cmdHello(false, true);
          for (const queued of this.queue.splice(0)) this.sendNow(queued);
          this.sendNow(hello);
          this.setOnline(true);
        },
        () => {
          // Attach failed (shell without the engine commands, e.g. a
          // browser that spoofed detection): allow a later retry.
          this.attachPromise = null;
          this.setOnline(false);
        },
      );
    }
    return this.attachPromise;
  }

  private sendNow(json: string): void {
    this.invoke('engine_cmd', { cmd: json }).then(
      (frames) => {
        if (!this.closed) this.feed(frames);
      },
      () => {
        // The engine is gone or the call failed: drop back offline; the
        // next poll re-attaches and the WS-style hello re-runs there.
        this.attached = false;
        this.attachPromise = null;
        this.setOnline(false);
      },
    );
  }

  private pollOnce(): void {
    if (this.polling || !this.attached || this.closed) return;
    this.polling = true;
    this.invoke('engine_poll').then(
      (frames) => {
        this.polling = false;
        if (!this.closed) this.feed(frames);
      },
      () => {
        this.polling = false;
        if (this.closed) return;
        this.attached = false;
        this.attachPromise = null;
        this.setOnline(false);
      },
    );
  }

  connect(): void {
    this.closed = false;
    // Swallow: failures surface as offline status + retries on the poll
    // loop, exactly like the WS transport's reconnect behaviour.
    this.ensureAttached().catch(() => {});
    if (this.timer === null) {
      this.timer = this.setTimer(() => {
        if (this.closed) return;
        if (!this.attached) {
          this.ensureAttached().catch(() => {});
          return;
        }
        this.pollOnce();
      }, this.pollMs);
    }
  }

  send(json: string): void {
    if (this.closed) return;
    if (!this.attached) {
      this.queue.push(json);
      this.ensureAttached().catch(() => {});
      return;
    }
    this.sendNow(json);
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
    this.closed = true;
    if (this.timer !== null) {
      try {
        this.clearTimer(this.timer);
      } catch {
        /* ignore */
      }
      this.timer = null;
    }
    this.queue.length = 0;
    this.attachPromise = null;
    const wasAttached = this.attached;
    this.attached = false;
    this.setOnline(false);
    if (wasAttached) {
      // Best-effort server-side detach; the engine sweeps stale clients
      // anyway, so a failure here is harmless.
      try {
        this.invoke('engine_detach').catch(() => {});
      } catch {
        /* sync throw (no shell): ignore */
      }
    }
  }
}

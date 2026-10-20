// W2a transport: the tiny socket seam under EngineWorld.
//
// `Transport` carries the engine Cmd/Event JSON (text) plus WS BINARY meter
// frames; `WsTransport` is the browser implementation (native WebSocket,
// binaryType arraybuffer, auto-reconnect with backoff, `hello
// {binary_meters:true}` on every connect). `ReplayTransport` feeds scripted
// golden frames for node tests. `TauriTransport` is the app-shell
// implementation: same Cmd/Event JSON over the embedded engine's IPC
// commands (`engine_attach`/`engine_cmd`/`engine_subscribe_channel`/
// `engine_subscribe_push`/`engine_detach` in `src-tauri/src/lib.rs`),
// pushed over an ordered Tauri `Channel` the transport creates and hands to
// `engine_subscribe_channel`: JSON control frames arrive as one batch
// (array of `Event::to_json` strings) plus hot meter frames as binary
// (`Event::to_binary`: meters/GR/taps, each its own `ArrayBuffer` — the
// same layout the WS relay uses; spectrum stays JSON, like WS), with
// `hello {binary_meters:true, taps:true}` negotiating the binary path.
// `engine_subscribe_push` + `openmix-event` listen remain only as the
// JSON-only fallback for shells without Channel support, and
// `engine_poll` as the final fallback drain for clients that cannot
// listen (node tests, foreign shells).

import { cmdHello } from './protocol.ts';

// Canonical probe lives in ./tauriEnv.ts (imported directly by the live
// singleton so it stays out of the boot chunk); re-exported here so
// existing `from './transport.ts'` importers keep working.
export { isTauriRuntime } from './tauriEnv.ts';

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
      const t = this.retryTimer;
      this.retryTimer = null;
      if (this.opts.clearTimer) this.opts.clearTimer(t);
      else clearTimeout(t as ReturnType<typeof setTimeout>);
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
        this.emit(v.buffer.slice(v.byteOffset, v.byteOffset + v.byteLength) as ArrayBuffer);
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
    const msg: ArrayBuffer = buf instanceof Uint8Array ? (buf.buffer.slice(buf.byteOffset, buf.byteOffset + buf.byteLength) as ArrayBuffer) : buf;
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
 * (`engine_attach`/`engine_cmd`/`engine_subscribe_channel`/
 * `engine_subscribe_push`/`engine_detach`
 * in `src-tauri/src/lib.rs`) instead of a WS relay. No host, no port, no
 * LAN server: discovery/scan runs in-process on the phone's WiFi.
 *
 * Framing is push-based: after attach the transport creates an ordered
 * Tauri `Channel`, hands it to `engine_subscribe_channel`, and feeds every
 * message straight through — no JS poll timer, so an idle phone holds no
 * 10 Hz IPC wakeup, and no `openmix-event` JSON-string eval round-trip.
 * The backend sends each JSON batch (array of `Event::to_json` strings,
 * control frames + spectrum) as one message, and each hot meter frame
 * (`Event::to_binary`: meters/GR/taps) as its own `ArrayBuffer` message
 * (same layout the WS relay uses). Binary frames carry the SPEC 2a
 * `seq`/`t_ms` header and are ingested via `parseBinary`, exactly like WS
 * binary. `send(json)` invokes `engine_cmd`; its return value is empty in
 * push mode (the same frames already arrived on the channel).
 *
 * Fallbacks, in order: the legacy `openmix-event` listen +
 * `engine_subscribe_push` (shells without Channel support, JSON-only),
 * then the old `engine_poll` loop (`pollMs`, default 100 ms) that feeds
 * `engine_cmd` return values — the exact pre-push behaviour (JSON-only,
 * never binary).
 *
 * The `invoke`/`listen`/`createChannel` functions are injectable so node
 * tests (and the debug fixture) can drive the transport without a Tauri
 * shell; by default they lazy-import `@tauri-apps/api/core` /
 * `@tauri-apps/api/event` on first use, so merely importing this module
 * in a browser never touches the Tauri runtime. `createChannel: null`
 * forces the legacy path (tests for the listen/poll fallbacks).
 */
export interface TauriInvokeFn {
  (cmd: string, args?: Record<string, unknown>): Promise<unknown>;
}

/** Event-listen shape (matches `@tauri-apps/api/event` `listen`). */
export interface TauriListenFn {
  (event: string, handler: (e: { payload: unknown }) => void): Promise<Unsub> | Unsub;
}

/**
 * Ordered push-channel shape (matches `Channel` from
 * `@tauri-apps/api/core`). The backend sends each JSON batch (array of
 * `Event::to_json` strings) as one message and each binary meter frame
 * (`Event::to_binary`) as one `ArrayBuffer` message; the transport feeds
 * both straight through (binary via `parseBinary` in EngineWorld). Only
 * `onmessage` is used — construction stays with the factory so tests can
 * inject a fake without the Tauri runtime.
 */
export interface TauriChannel {
  onmessage: ((msg: unknown) => void) | null;
}

/**
 * Channel factory (default: lazy `new Channel()` from
 * `@tauri-apps/api/core`). `null` forces the legacy listen/poll fallbacks.
 */
export type TauriChannelFactory = () => TauriChannel;

/** Backend event carrying engine Event JSON frames (see `OPENMIX_EVENT`). */
export const OPENMIX_EVENT = 'openmix-event';

export interface TauriTransportOptions {
  /** IPC function (default: lazy `@tauri-apps/api/core` invoke). */
  invoke?: TauriInvokeFn;
  /** Event-listen function (default: lazy `@tauri-apps/api/event` listen).
   * `null` forces the `engine_poll` fallback loop (node tests that only
   * inject `invoke` rely on the default auto-detect, not on this). */
  listen?: TauriListenFn | null;
  /** Channel factory (default: lazy `new Channel()` from
   * `@tauri-apps/api/core`). `null` forces the legacy listen/poll
   * fallbacks (tests for those paths). */
  createChannel?: TauriChannelFactory | null;
  /** Background poll interval in ms for the fallback loop (default 100). */
  pollMs?: number;
  /** Timer injection for tests (default setInterval/clearInterval). */
  setTimer?: (fn: () => void, ms: number) => unknown;
  clearTimer?: (t: unknown) => void;
}

const TAURI_POLL_MS = 100;

export class TauriTransport implements Transport {
  readonly url = 'tauri://engine';
  private readonly invokeOpt: TauriInvokeFn | null;
  private readonly listenOpt: TauriListenFn | null | undefined;
  private readonly channelFactory: TauriChannelFactory | null | undefined;
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
  /** True once Channel (or legacy event) push is live: `engine_cmd`
   * returns are ignored (the same frames already arrived on the push
   * path). */
  private pushMode = false;
  private unlisten: Unsub | null = null;
  private channel: TauriChannel | null = null;
  private pushSetup: Promise<void> | null = null;
  private timer: unknown = null;
  private attachPromise: Promise<void> | null = null;

  constructor(opts: TauriTransportOptions = {}) {
    this.invokeOpt = opts.invoke ?? null;
    this.listenOpt = opts.listen;
    this.channelFactory = opts.createChannel;
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

  private emitMsg(msg: TransportMessage): void {
    this.msgCbs.forEach((cb) => {
      try {
        cb(msg);
      } catch {
        /* ignore */
      }
    });
  }

  private feed(frames: unknown): void {
    // Channel messages are one of: a JSON batch (array of `Event::to_json`
    // strings), a single binary meter frame (`ArrayBuffer` from a `Raw`
    // send, or a `Uint8Array` view over one), or — on Android, where Raw
    // IPC bodies are unsupported — a JSON number array carrying the same
    // bytes (Tauri converts Raw to a number array on mobile). Strings and
    // binary both flow to EngineWorld, which ingests text via `parseEvent`
    // and binary via `parseBinary` (SPEC 2a `seq`/`t_ms` header intact).
    if (frames instanceof ArrayBuffer) {
      this.emitMsg(frames);
      return;
    }
    if (typeof ArrayBuffer !== 'undefined' && ArrayBuffer.isView(frames)) {
      const v = frames as Uint8Array;
      this.emitMsg((v.buffer.slice(v.byteOffset, v.byteOffset + v.byteLength) as ArrayBuffer));
      return;
    }
    if (!Array.isArray(frames)) return;
    if (frames.length > 0 && typeof frames[0] === 'number') {
      const bytes = Uint8Array.from(frames as number[]);
      this.emitMsg((bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength) as ArrayBuffer));
      return;
    }
    for (const f of frames) {
      if (typeof f !== 'string') continue;
      const msg: TransportMessage = f;
      this.emitMsg(msg);
    }
  }

  private ensureAttached(): Promise<void> {
    if (this.attached) return Promise.resolve();
    if (!this.attachPromise) {
      this.attachPromise = this.invoke('engine_attach', { label: 'ui' }).then(
        async () => {
          if (this.closed) return;
          // Push first, hello second: every frame sent after this point is
          // either pushed (and its `engine_cmd` return ignored) or, on
          // fallback shells, returned (and fed). Subscribing before the
          // first `send` keeps the two paths from ever double-feeding.
          await this.setupPush();
          if (this.closed) return;
          this.attached = true;
          // Same hello the WS path sends on every (re)connect: binary
          // meters + taps now ride the ordered Channel (`Raw` binary
          // frames, SPEC 2a `seq`/`t_ms` header), so EngineWorld gets the
          // full meter fan-out as binary, exactly like WS.
          const hello = cmdHello(true, true);
          for (const queued of this.queue.splice(0)) this.sendNow(queued);
          this.sendNow(hello);
          this.setOnline(true);
        },
        () => {
          // Attach failed (shell without the engine commands, e.g. a
          // browser that spoofed detection): allow a later retry.
          this.attachPromise = null;
          this.pushSetup = null;
          this.setOnline(false);
        },
      );
    }
    return this.attachPromise;
  }

  /** Bring up push fan-out: ordered Channel first
   * (`engine_subscribe_channel`), then the legacy `openmix-event` listen
   * (`engine_subscribe_push`), then the `engine_poll` fallback loop.
   * Resolves in all cases — push is best-effort, connectivity is not. */
  private setupPush(): Promise<void> {
    if (this.pushSetup) return this.pushSetup;
    this.pushSetup = (async () => {
      if (this.closed || this.unlisten || this.channel) return;
      // Fast path: ordered IPC Channel (no event eval, no poll timer).
      if (await this.setupChannelPush()) return;
      if (this.closed) return;
      // Legacy path: `openmix-event` listen + subscribe.
      let listenFn: TauriListenFn | null = null;
      if (this.listenOpt !== undefined) {
        listenFn = this.listenOpt;
      } else {
        try {
          const m = await import('@tauri-apps/api/event');
          listenFn = m.listen as TauriListenFn;
        } catch {
          listenFn = null;
        }
      }
      if (!listenFn || this.closed) {
        this.startPollLoop();
        return;
      }
      try {
        const un = await listenFn(OPENMIX_EVENT, (e) => this.feed(e.payload));
        if (this.closed) {
          try {
            un();
          } catch {
            /* ignore */
          }
          return;
        }
        this.unlisten = un;
        await this.invoke('engine_subscribe_push');
        this.pushMode = true;
        this.stopPollLoop();
      } catch {
        this.unlisten = null;
        this.pushMode = false;
        this.startPollLoop();
      }
    })();
    return this.pushSetup;
  }

  /** Create the push Channel and hand it to `engine_subscribe_channel`.
   * Returns true once push mode is live over the channel. Any failure
   * (no Tauri runtime, factory threw, backend rejected) returns false so
   * the caller falls through to the legacy paths. */
  private async setupChannelPush(): Promise<boolean> {
    let ch: TauriChannel | null = null;
    if (this.channelFactory !== undefined) {
      // Explicit injection: `null` forces the legacy paths (tests for the
      // fallbacks); otherwise a throw means "no channel here".
      if (this.channelFactory === null) return false;
      try {
        ch = this.channelFactory();
      } catch {
        return false;
      }
    } else {
      try {
        const m = await import('@tauri-apps/api/core');
        const Ctor = (m as { Channel?: new () => TauriChannel }).Channel;
        if (typeof Ctor !== 'function') return false;
        ch = new Ctor();
      } catch {
        return false;
      }
    }
    if (!ch || this.closed) return false;
    ch.onmessage = (frames) => this.feed(frames);
    try {
      await this.invoke('engine_subscribe_channel', { onEvent: ch });
    } catch {
      return false;
    }
    if (this.closed) return false;
    this.channel = ch;
    this.pushMode = true;
    this.stopPollLoop();
    return true;
  }

  private startPollLoop(): void {
    if (this.timer === null && !this.closed) {
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

  private stopPollLoop(): void {
    if (this.timer !== null) {
      try {
        this.clearTimer(this.timer);
      } catch {
        /* ignore */
      }
      this.timer = null;
    }
  }

  private sendNow(json: string): void {
    this.invoke('engine_cmd', { cmd: json }).then(
      (frames) => {
        // Push mode: the same batch already arrived on the push path
        // (Channel or legacy event) — feeding the return too would
        // duplicate every frame. Fallback mode: the return value is the
        // only copy.
        if (!this.closed && !this.pushMode) this.feed(frames);
      },
      () => {
        // The engine is gone or the call failed: drop back offline; the
        // next poll re-attaches and the WS-style hello re-runs there.
        this.attached = false;
        this.attachPromise = null;
        this.pushSetup = null;
        this.pushMode = false;
        this.channel = null;
        this.setOnline(false);
      },
    );
  }

  private pollOnce(): void {
    if (this.polling || this.pushMode || !this.attached || this.closed) return;
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
        this.pushSetup = null;
        this.setOnline(false);
      },
    );
  }

  connect(): void {
    this.closed = false;
    // Swallow: failures surface as offline status + retries on the
    // fallback poll loop, exactly like the WS transport's reconnect
    // behaviour. The loop itself starts in setupPush (fallback path);
    // push shells hold no timer at all.
    this.ensureAttached().catch(() => {});
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
    this.stopPollLoop();
    this.pushSetup = null;
    this.pushMode = false;
    // Dropping our side stops delivery; the `engine_detach` below drops
    // the client the backend drains, so its stored clone goes quiet, and
    // the next subscribe replaces it.
    this.channel = null;
    if (this.unlisten) {
      try {
        this.unlisten();
      } catch {
        /* ignore */
      }
      this.unlisten = null;
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

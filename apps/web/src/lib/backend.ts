// openmix web backend interface — two transports, same methods.
//
//   connect(driver, addr)  -> Promise<string>  ("connected via sim")
//   setFader(ch, v01)      -> Promise<void>     (v01 in 0..1)
//   setMute(ch, muted)     -> Promise<void>
//   setParam(ch, field, v) -> Promise<void>     (number|boolean|string)
//   onState(cb)            -> unsub             (cb(strips [{ch,name,fader,muted}]))
//   onMeters(cb)           -> unsub             (cb(levels [0..1]))
//   onSpectrum(cb)         -> unsub             (cb({ch,bins}) — no source yet)
//
// Plus optional onStatus(cb) -> unsub (cb(true|false) online flag).
// Auto-select: Tauri IPC when running inside Tauri, else WebSocket relay.
//
// WS protocol (port :8080): host -> {cmd:'connect',driver,addr},
// {cmd:'fader',ch,value}, {cmd:'mute',ch,muted}, {cmd:'param',ch,field,value};
// relay -> {type:'state',strips:[...]}, {type:'meters',levels:[...]}.
// Future: {type:'spectrum',ch,bins:[...]} (serve sends none yet; see
// Spectrum.svelte header). Both backends expose onSpectrum(cb) so the UI
// can subscribe today and light up when the relay starts sending spectra.
// Ported from ../../ui/api.js — keep the two in sync contract-wise.

export interface Strip {
  ch: number;
  name: string;
  fader: number;
  muted: boolean;
  main?: boolean;
}

/** Layer-switcher marker for the MAIN pseudo-strip (no backend fader yet). */
export interface MainStrip {
  readonly ch: -1;
  name: string;
}

export type MeterLevels = number[];
export type ParamValue = number | boolean | string;
export type Unsub = () => void;

export interface SpectrumEvent {
  ch: number;
  bins: number[];
}

/** Relay -> UI messages (WS JSON protocol, port :8080). */
export type WsMsg =
  | { type: 'state'; strips: Strip[] }
  | { type: 'meters'; levels: number[] }
  | { type: 'spectrum'; ch: number; bins: number[] };

/** UI -> relay commands. */
export type WsCmd =
  | { cmd: 'connect'; driver: string; addr: string }
  | { cmd: 'fader'; ch: number; value: number }
  | { cmd: 'mute'; ch: number; muted: boolean }
  | { cmd: 'param'; ch: number; field: string; value: ParamValue };

export interface Backend {
  readonly kind: 'tauri' | 'ws';
  /** WsBackend relay URL (undefined on the Tauri transport). */
  readonly url?: string;
  connect(driver: string, addr: string): Promise<string>;
  setFader(ch: number, v01: number): Promise<void>;
  setMute(ch: number, muted: boolean): Promise<void>;
  setParam(ch: number, field: string, v: ParamValue): Promise<void>;
  onState(cb: (strips: Strip[]) => void): Unsub;
  onMeters(cb: (levels: MeterLevels) => void): Unsub;
  onSpectrum(cb: (evt: SpectrumEvent) => void): Unsub;
  onStatus(cb: (online: boolean) => void): Unsub;
  close?(): void;
}

function sub<T>(list: Set<(v: T) => void>, cb: (v: T) => void): Unsub {
  list.add(cb);
  return () => {
    list.delete(cb);
  };
}

/** Normalize one wire strip into a Strip (tolerates `mute` vs `muted`). */
function normStrip(s: unknown, i: number): Strip {
  // Minimal documented cast: JSON wire shape validated field-by-field below.
  const rec = (s !== null && typeof s === 'object' ? s : {}) as Record<string, unknown>;
  const chRaw = rec['ch'];
  const ch = typeof chRaw === 'number' && Number.isFinite(chRaw) ? chRaw : i;
  const nameRaw = rec['name'];
  const m = rec['muted'] ?? rec['mute'] ?? false;
  return {
    ch,
    name: nameRaw === undefined || nameRaw === null ? `CH${String(ch + 1).padStart(2, '0')}` : String(nameRaw),
    fader: Number(rec['fader'] ?? 0.75),
    muted: !!m,
  };
}

/* ---------------- Tauri (desktop/mobile webview) ---------------- */

interface TauriCore {
  invoke(cmd: string, args?: Record<string, unknown>): Promise<unknown>;
}

declare global {
  interface Window {
    __TAURI__?: { core: TauriCore };
  }
}

export class TauriBackend implements Backend {
  readonly kind = 'tauri' as const;
  private online = false;
  private readonly stateCbs = new Set<(strips: Strip[]) => void>();
  private readonly meterCbs = new Set<(levels: MeterLevels) => void>();
  private readonly statusCbs = new Set<(online: boolean) => void>();
  private readonly spectrumCbs = new Set<(evt: SpectrumEvent) => void>();
  private strips: Strip[] = Array.from({ length: 8 }, (_, i) => ({
    ch: i,
    name: `CH${String(i + 1).padStart(2, '0')}`,
    fader: 0.75,
    muted: false,
  }));
  private lastLevels: MeterLevels = this.strips.map(() => 0);
  private pollTimer: number | null = null;
  private readonly invoke: TauriCore['invoke'];

  constructor() {
    const core = window.__TAURI__?.core;
    if (!core) throw new Error('tauri runtime not found');
    this.invoke = core.invoke.bind(core);
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

  private emitState(): void {
    const snap = this.strips.map((s) => ({ ...s }));
    this.stateCbs.forEach((cb) => {
      try {
        cb(snap);
      } catch {
        /* ignore */
      }
    });
  }

  async connect(driver: string, addr: string): Promise<string> {
    const msg = await this.invoke('connect', { driver, addr: addr || '' });
    this.setOnline(true);
    this.emitState();
    return String(msg);
  }

  async setFader(ch: number, v01: number): Promise<void> {
    await this.invoke('set_fader', { ch, value: v01 });
    if (this.strips[ch]) {
      this.strips[ch].fader = v01;
      this.emitState();
    }
  }

  async setMute(ch: number, muted: boolean): Promise<void> {
    await this.invoke('set_mute', { ch, muted });
    if (this.strips[ch]) {
      this.strips[ch].muted = muted;
      this.emitState();
    }
  }

  // No `set_param` command in the Tauri shell yet: the invoke rejects and
  // the detail panel hides that control (unsupported-gracefully).
  async setParam(ch: number, field: string, v: ParamValue): Promise<void> {
    await this.invoke('set_param', { ch, field, value: v });
  }

  onState(cb: (strips: Strip[]) => void): Unsub {
    const un = sub(this.stateCbs, cb);
    try {
      cb(this.strips.map((s) => ({ ...s })));
    } catch {
      /* ignore */
    }
    return un;
  }

  onMeters(cb: (levels: MeterLevels) => void): Unsub {
    const un = sub(this.meterCbs, cb);
    if (this.pollTimer === null) {
      this.pollTimer = window.setInterval(() => {
        if (!this.meterCbs.size) return;
        void (async () => {
          try {
            const raw: unknown = await this.invoke('get_meters');
            if (!Array.isArray(raw)) return;
            const levels: MeterLevels = raw.map((v: unknown) => Number(v) || 0);
            this.lastLevels = levels;
            this.meterCbs.forEach((f) => {
              try {
                f(levels);
              } catch {
                /* ignore */
              }
            });
          } catch {
            /* backend away: keep last frame */
          }
        })();
      }, 120);
    }
    try {
      cb(this.lastLevels);
    } catch {
      /* ignore */
    }
    return un;
  }

  onStatus(cb: (online: boolean) => void): Unsub {
    const un = sub(this.statusCbs, cb);
    try {
      cb(this.online);
    } catch {
      /* ignore */
    }
    return un;
  }

  // No spectrum source in the Tauri shell yet: subscribe works, nothing fires.
  onSpectrum(cb: (evt: SpectrumEvent) => void): Unsub {
    return sub(this.spectrumCbs, cb);
  }
}

/* ---------------- WebSocket relay (any browser) ---------------- */

export function wsUrlFromQuery(defHost?: string): string {
  const q = new URLSearchParams(location.search);
  const s = (q.get('server') || '').trim();
  if (/^wss?:\/\//i.test(s)) return s;
  if (s) return `ws://${s.replace(/^https?:\/\//, '')}`;
  const host = defHost || location.hostname || '127.0.0.1';
  return `ws://${host}:8080`;
}

export class WsBackend implements Backend {
  readonly kind = 'ws' as const;
  readonly url: string;
  private online = false;
  private readonly stateCbs = new Set<(strips: Strip[]) => void>();
  private readonly meterCbs = new Set<(levels: MeterLevels) => void>();
  private readonly statusCbs = new Set<(online: boolean) => void>();
  private readonly spectrumCbs = new Set<(evt: SpectrumEvent) => void>();
  private ws: WebSocket | null = null;
  private openPromise: Promise<string> | null = null;
  private readonly queue: string[] = [];
  private driver = 'sim';
  private addr = '';
  private wantReconnect = true;
  private reTimer: number | null = null;

  constructor(url?: string) {
    this.url = url || wsUrlFromQuery();
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

  private ensure(): Promise<string> {
    if (
      this.ws &&
      (this.ws.readyState === WebSocket.OPEN || this.ws.readyState === WebSocket.CONNECTING) &&
      this.openPromise
    ) {
      return this.openPromise;
    }
    const ws = new WebSocket(this.url);
    this.ws = ws;
    this.openPromise = new Promise<string>((resolve, reject) => {
      const to = window.setTimeout(() => reject(new Error(`ws timeout ${this.url}`)), 5000);
      ws.onopen = () => {
        window.clearTimeout(to);
        this.setOnline(true);
        for (const m of this.queue.splice(0)) ws.send(m);
        if (this.driver) this.send({ cmd: 'connect', driver: this.driver, addr: this.addr });
        resolve(`connected via ws ${this.url}`);
      };
      ws.onerror = () => {
        /* onclose reports */
      };
      ws.onclose = () => {
        window.clearTimeout(to);
        this.setOnline(false);
        this.ws = null;
        if (this.wantReconnect && this.reTimer === null) {
          this.reTimer = window.setTimeout(() => {
            this.reTimer = null;
            this.ensure().catch(() => {});
          }, 1500);
        }
      };
      ws.onmessage = (ev: MessageEvent) => this.handle(ev.data);
    });
    return this.openPromise;
  }

  private send(obj: WsCmd): void {
    const msg = JSON.stringify(obj);
    if (this.ws && this.ws.readyState === WebSocket.OPEN) this.ws.send(msg);
    else this.queue.push(msg);
  }

  private handle(raw: unknown): void {
    if (typeof raw !== 'string') return;
    let m: unknown;
    try {
      m = JSON.parse(raw);
    } catch {
      return;
    }
    if (m === null || typeof m !== 'object' || Array.isArray(m)) return;
    // Minimal documented cast: wire shape validated field-by-field below.
    const rec = m as Record<string, unknown>;
    const t = rec['type'];
    if (t === 'state' && Array.isArray(rec['strips'])) {
      const strips: Strip[] = rec['strips'].map((s: unknown, i: number) => normStrip(s, i));
      this.stateCbs.forEach((cb) => {
        try {
          cb(strips);
        } catch {
          /* ignore */
        }
      });
    } else if (t === 'meters' && Array.isArray(rec['levels'])) {
      const levels: MeterLevels = rec['levels'].map((v: unknown) => Number(v) || 0);
      this.meterCbs.forEach((cb) => {
        try {
          cb(levels);
        } catch {
          /* ignore */
        }
      });
    } else if (t === 'spectrum' && typeof rec['ch'] === 'number' && Array.isArray(rec['bins'])) {
      const evt: SpectrumEvent = {
        ch: rec['ch'],
        bins: rec['bins'].map((v: unknown) => Number(v) || 0),
      };
      this.spectrumCbs.forEach((cb) => {
        try {
          cb(evt);
        } catch {
          /* ignore */
        }
      });
    }
  }

  async connect(driver: string, addr: string): Promise<string> {
    this.driver = driver;
    this.addr = addr || '';
    return this.ensure();
  }

  async setFader(ch: number, v01: number): Promise<void> {
    this.send({ cmd: 'fader', ch, value: v01 });
  }

  async setMute(ch: number, muted: boolean): Promise<void> {
    this.send({ cmd: 'mute', ch, muted });
  }

  async setParam(ch: number, field: string, v: ParamValue): Promise<void> {
    this.send({ cmd: 'param', ch, field, value: v });
  }

  onState(cb: (strips: Strip[]) => void): Unsub {
    return sub(this.stateCbs, cb);
  }
  onMeters(cb: (levels: MeterLevels) => void): Unsub {
    return sub(this.meterCbs, cb);
  }
  onSpectrum(cb: (evt: SpectrumEvent) => void): Unsub {
    return sub(this.spectrumCbs, cb);
  }
  onStatus(cb: (online: boolean) => void): Unsub {
    const un = sub(this.statusCbs, cb);
    try {
      cb(this.online);
    } catch {
      /* ignore */
    }
    return un;
  }

  close(): void {
    this.wantReconnect = false;
    if (this.reTimer !== null) window.clearTimeout(this.reTimer);
    if (this.ws) this.ws.close();
  }
}

/* ---------------- auto-select ---------------- */

export function hasTauri(): boolean {
  return !!(window.__TAURI__ && window.__TAURI__.core && typeof window.__TAURI__.core.invoke === 'function');
}

export function selectBackend(): Backend {
  return hasTauri() ? new TauriBackend() : new WsBackend();
}

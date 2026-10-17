// openmix web backend interface — two transports, same methods.
//
//   connect(driver, addr)  -> Promise<string>  ("connected via sim")
//   setFader(ch, v01)      -> Promise<void>     (v01 in 0..1)
//   setMainFader(v01)      -> Promise<void>     (LR main, v01 in 0..1)
//   setMute(ch, muted)     -> Promise<void>
//   setParam(ch, field, v) -> Promise<void>     (number|boolean|string)
//   onState(cb)            -> unsub             (cb(strips [{ch,name,fader,muted}]))
//   onMain(cb)             -> unsub             (cb(mainFader number|null; null until first state / driver without main))
//   onMeters(cb)           -> unsub             (cb(levels [0..1]))
//   onSpectrum(cb)         -> unsub             (cb({ch,bins}))
//   onGr(cb)               -> unsub             (cb({ch,gate,comp}) — dB reduction >= 0)
//
// Plus optional onStatus(cb) -> unsub (cb(true|false) online flag).
// Auto-select: Tauri IPC when running inside Tauri, else WebSocket relay.
//
// WS protocol (port :8080): host -> {cmd:'connect',driver,addr},
// {cmd:'fader',ch,value}, {cmd:'main_fader',value}, {cmd:'mute',ch,muted},
// {cmd:'param',ch,field,value},
// {cmd:'list_shows'}, {cmd:'capture_scene',show,name},
// {cmd:'recall_scene',show,name,fade_ms}, {cmd:'delete_scene',show,name};
// relay -> {type:'you',id} (unicast self id on connect + every connect
// cmd, before state), {type:'state',strips:[...],main_fader:number|null}, {type:'meters',levels:[...]},
// {type:'spectrum',ch,bins:[...]}, {type:'gr',ch,gate,comp} (~5 Hz),
// {type:'shows',shows:[...]} (full library; sent on connect, as the
// list_shows reply, and broadcast after capture/delete).
// Both backends expose onSpectrum/onGr so the UI subscribes once and lights
// up as the relay sends spectra/GR frames.
// Ported from ../../ui/api.js — keep the two in sync contract-wise.

export interface Strip {
  ch: number;
  name: string;
  fader: number;
  muted: boolean;
  main?: boolean;
}

/** Layer-switcher marker for the MAIN pseudo-strip (driver-backed LR main). */
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

/** Gain-reduction frame: dB reduction >= 0 per stage (0 = no reduction). */
export interface GrEvent {
  ch: number;
  gate: number;
  comp: number;
}

/** `param` broadcast frame (relay fans every `param` cmd out to all clients). */
export interface ParamEvent {
  ch: number;
  field: string;
  value: ParamValue;
  by?: string;
}

/** One strip in a captured scene (cf. core SceneStrip). */
export interface ShowSceneStrip {
  ch: number;
  name: string;
  fader: number;
  muted: boolean;
}

/** Named, recallable mixer state (cf. core Scene; notes stay empty). */
export interface ShowScene {
  name: string;
  strips: ShowSceneStrip[];
  main_fader: number | null;
}

/** Ordered library of scenes (cf. core Show). */
export interface ShowLib {
  name: string;
  scenes: ShowScene[];
}

/** One strip lock holder: channel -> holder client id (e.g. "client-3"). */
export type LockHolds = Record<number, string>;

/** Lock/write denial from the relay (`{"type":"error","locked_by":…}`). */
export interface LockDeny {
  ch: number | null;
  lockedBy: string | null;
  message: string;
}

/**
 * Client roster from the relay (`{"type":"clients","count","writers"}`).
 * `writers` holds the ids with a mutation in the last 10 s (server-sorted).
 * The frame carries ids only — labels stay server-side. `selfId` is the
 * authoritative id from the unicast `{"type":"you","id"}` frame (null until
 * the server sends it; old servers without `you` fall back to `by`-echo
 * guessing after our first write).
 */
export interface ClientsInfo {
  count: number;
  writers: string[];
  selfId: string | null;
}

  /** Relay -> UI messages (WS JSON protocol, port :8080).
   * `you` is the unicast authoritative self id (connect + every `connect`
   * cmd, before `state`); `clients` is the broadcast roster. */
export type WsMsg =
  | { type: 'state'; strips: Strip[]; main_fader?: number | null }
  | { type: 'meters'; levels: number[] }
  | { type: 'spectrum'; ch: number; bins: number[] }
  | { type: 'gr'; ch: number; gate: number; comp: number }
  | { type: 'param'; ch: number; field: string; value: ParamValue; by?: string }
  | { type: 'shows'; shows: ShowLib[] }
  | { type: 'locks'; holds: Record<string, string> }
  | { type: 'clients'; count: number; writers: string[] }
 | { type: 'you'; id: string }
  | { type: 'error'; message?: string; locked_by?: string; ch?: number }
  | { type: 'recalled'; overrode_locks?: number[] };

/** UI -> relay commands. */
export type WsCmd =
  | { cmd: 'connect'; driver: string; addr: string }
  | { cmd: 'fader'; ch: number; value: number }
  | { cmd: 'main_fader'; value: number }
  | { cmd: 'mute'; ch: number; muted: boolean }
  | { cmd: 'param'; ch: number; field: string; value: ParamValue }
  | { cmd: 'list_shows' }
  | { cmd: 'capture_scene'; show: string; name: string }
  | { cmd: 'recall_scene'; show: string; name: string; fade_ms: number | null }
  | { cmd: 'delete_scene'; show: string; name: string }
  | { cmd: 'lock'; ch: number }
  | { cmd: 'unlock'; ch: number };

export interface Backend {
  readonly kind: 'tauri' | 'ws';
  /** WsBackend relay URL (undefined on the Tauri transport). */
  readonly url?: string;
  connect(driver: string, addr: string): Promise<string>;
  setFader(ch: number, v01: number): Promise<void>;
  /** LR main fader (0..1; rejects when the driver has no main). */
  setMainFader(v01: number): Promise<void>;
  setMute(ch: number, muted: boolean): Promise<void>;
  setParam(ch: number, field: string, v: ParamValue): Promise<void>;
  /** Full server show library (source of truth while connected). */
  listShows(): Promise<ShowLib[]>;
  /** Snapshot live state into show/name (upsert; creates the show server-side). */
  captureScene(show: string, name: string): Promise<void>;
  /** Server-side recall with a fader fade over fadeMs (default 400; 0 jumps). */
  recallScene(show: string, name: string, fadeMs?: number): Promise<void>;
  /** Remove name from show's library. */
  deleteScene(show: string, name: string): Promise<void>;
  /** Claim a strip lock (no-op on transports without locks). */
  lockStrip(ch: number): Promise<void>;
  /** Release a strip lock (no-op on transports without locks). */
  unlockStrip(ch: number): Promise<void>;
  onShows(cb: (shows: ShowLib[]) => void): Unsub;
  onState(cb: (strips: Strip[]) => void): Unsub;
  /** LR main fader (null until the first state frame / driver without main). */
  onMain(cb: (main: number | null) => void): Unsub;
  onMeters(cb: (levels: MeterLevels) => void): Unsub;
  onSpectrum(cb: (evt: SpectrumEvent) => void): Unsub;
  onGr(cb: (evt: GrEvent) => void): Unsub;
  onStatus(cb: (online: boolean) => void): Unsub;
  /** `param` broadcast (e.g. musician CALL FOH `attention`); empty on old servers. */
  onParam(cb: (evt: ParamEvent) => void): Unsub;
  /** Strip-lock table (empty when the server has no locks support). */
  onLocks(cb: (holds: LockHolds) => void): Unsub;
  /** Lock/write denials (`locked_by` holder id when the server names one). */
  onDeny(cb: (deny: LockDeny) => void): Unsub;
  /**
   * Client roster (relay only). Absent on transports without a presence
   * feed (Tauri) — callers must null-check (`backend.onClients?.(...)`).
   */
  onClients?(cb: (info: ClientsInfo) => void): Unsub;
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

/** Normalize one wire show strip into a ShowSceneStrip (null when unusable). */
function normShowStrip(s: unknown): ShowSceneStrip | null {
  if (s === null || typeof s !== 'object') return null;
  const rec = (s !== null && typeof s === 'object' ? s : {}) as Record<string, unknown>;
  if (typeof rec['ch'] !== 'number' || !Number.isFinite(rec['ch'])) return null;
  const f = Number(rec['fader'] ?? 0.75);
  return {
    ch: rec['ch'],
    name: typeof rec['name'] === 'string' ? rec['name'] : `CH${rec['ch'] + 1}`,
    fader: Number.isFinite(f) ? Math.min(1, Math.max(0, f)) : 0.75,
    muted: !!rec['muted'],
  };
}

/** Normalize one wire show; null when the shape is unusable. */
export function normShow(v: unknown): ShowLib | null {
  if (v === null || typeof v !== 'object') return null;
  const r = v as Record<string, unknown>;
  if (typeof r['name'] !== 'string' || !Array.isArray(r['scenes'])) return null;
  const scenes: ShowScene[] = [];
  for (const e of r['scenes']) {
    if (e === null || typeof e !== 'object') return null;
    const er = e as Record<string, unknown>;
    if (typeof er['name'] !== 'string' || !Array.isArray(er['strips'])) return null;
    const strips: ShowSceneStrip[] = [];
    for (const s of er['strips']) {
      const ok = normShowStrip(s);
      if (!ok) return null;
      strips.push(ok);
    }
    const mf = er['main_fader'];
    scenes.push({
      name: er['name'],
      strips,
      main_fader: typeof mf === 'number' && Number.isFinite(mf) ? Math.min(1, Math.max(0, mf)) : null,
    });
  }
  return { name: r['name'], scenes };
}

/** Normalize a wire `shows` array, dropping unusable entries. */
export function normShows(v: unknown): ShowLib[] {
  if (!Array.isArray(v)) return [];
  const out: ShowLib[] = [];
  for (const e of v) {
    const s = normShow(e);
    if (s && s.name) out.push(s);
  }
  return out;
}

/** Deep copy a show library (snapshot isolation for subscribers). */
function copyShows(libs: ShowLib[]): ShowLib[] {
  return libs.map((s) => ({
    name: s.name,
    scenes: s.scenes.map((sc) => ({ ...sc, strips: sc.strips.map((st) => ({ ...st })) })),
  }));
}

/* ---------------- Tauri (desktop/mobile webview) ---------------- */

/** Show name the Tauri shell files its flat scene list under (cf. main.rs). */
const TAURI_SHOW = 'openmix';

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
  private readonly grCbs = new Set<(evt: GrEvent) => void>();
  private readonly showsCbs = new Set<(shows: ShowLib[]) => void>();
  /** LR main fader mirror (null until the shell reports one). */
  private main: number | null = null;
  private readonly mainCbs = new Set<(main: number | null) => void>();
  /** Local fallback library (used when the shell has no scene ops). */
  private localShows: ShowLib[] = [{ name: TAURI_SHOW, scenes: [] }];
  private strips: Strip[] = Array.from({ length: 8 }, (_, i) => ({
    ch: i,
    name: `CH${String(i + 1).padStart(2, '0')}`,
    fader: 0.75,
    muted: false,
  }));
  private lastLevels: MeterLevels = this.strips.map(() => 0);
  private pollTimer: number | null = null;
  /** ~5 Hz spectrum/GR poll (pull mirror of the WS push; see ensureAvPoll). */
  private avTimer: number | null = null;
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

  private emitMain(): void {
    const v = this.main;
    this.mainCbs.forEach((cb) => {
      try {
        cb(v);
      } catch {
        /* ignore */
      }
    });
  }

  async connect(driver: string, addr: string): Promise<string> {
    const msg = await this.invoke('connect', { driver, addr: addr || '' });
    this.setOnline(true);
    try {
      const raw = ((await this.invoke('get_state')) ?? {}) as Record<string, unknown>;
      if (Array.isArray(raw['strips'])) {
        const strips = (raw['strips'] as unknown[]).map((s, i) => normStrip(s, i));
        if (strips.length) {
          this.strips = strips;
          this.lastLevels = strips.map(() => 0);
        }
      }
      const mf = raw['main_fader'];
      this.main =
        typeof mf === 'number' && Number.isFinite(mf) ? Math.min(1, Math.max(0, mf)) : null;
    } catch {
      /* shell without get_state: keep the local default mirror */
    }
    if (this.main === null) {
      try {
        const raw: unknown = await this.invoke('get_main_fader');
        this.main =
          typeof raw === 'number' && Number.isFinite(raw) ? Math.min(1, Math.max(0, raw)) : null;
      } catch {
        /* shell without a main command: stays null */
      }
    }
    this.emitState();
    this.emitMain();
    void this.refreshShows();
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

  async setMainFader(v01: number): Promise<void> {
    await this.invoke('set_main_fader', { value: v01 });
    this.main = v01;
    this.emitMain();
  }

  // `set_param` maps to the shell's `set_param` command (same
  // number|boolean|string encoding as the WS `param` cmd).
  async setParam(ch: number, field: string, v: ParamValue): Promise<void> {
    await this.invoke('set_param', { ch, field, value: v });
  }

  private emitShows(): void {
    const snap = copyShows(this.localShows);
    this.showsCbs.forEach((cb) => {
      try {
        cb(snap);
      } catch {
        /* ignore */
      }
    });
  }

  /**
   * Rebuild the library from the shell's flat scene list
   * (`list_scenes` + `get_scene` per name, filed under TAURI_SHOW).
   * A shell without scene ops (or away) leaves the local fallback alone.
   */
  private async refreshShows(): Promise<ShowLib[]> {
    try {
      const names: unknown = await this.invoke('list_scenes');
      if (!Array.isArray(names)) throw new Error('list_scenes shape');
      const scenes: ShowScene[] = [];
      for (const n of names) {
        if (typeof n !== 'string') continue;
        try {
          const raw = ((await this.invoke('get_scene', { name: n })) ?? {}) as Record<string, unknown>;
          const scene = normShow({
            name: TAURI_SHOW,
            scenes: [{ name: n, strips: raw['strips'], main_fader: raw['main_fader'] }],
          })?.scenes[0];
          if (scene) scenes.push(scene);
        } catch {
          /* skip one bad scene, keep the rest */
        }
      }
      this.localShows = [{ name: TAURI_SHOW, scenes }];
    } catch {
      /* shell without scene ops: keep the local fallback */
    }
    this.emitShows();
    return copyShows(this.localShows);
  }

  async listShows(): Promise<ShowLib[]> {
    return this.refreshShows();
  }

  async captureScene(show: string, name: string): Promise<void> {
    const clean = name.trim().slice(0, 32);
    if (!clean) throw new Error('scene name must not be empty');
    try {
      await this.invoke('capture_scene', { name: clean });
      await this.refreshShows();
      return;
    } catch {
      /* shell without scene ops: snapshot the local strip mirror */
    }
    const snap: ShowScene = {
      name: clean,
      strips: this.strips.map((s) => ({ ch: s.ch, name: s.name, fader: s.fader, muted: s.muted })),
      main_fader: null,
    };
    const slotName = show.trim() || TAURI_SHOW;
    const libs = copyShows(this.localShows);
    let slot = libs.find((s) => s.name === slotName);
    if (!slot) {
      slot = { name: slotName, scenes: [] };
      libs.push(slot);
    }
    const idx = slot.scenes.findIndex((s) => s.name === clean);
    if (idx >= 0) slot.scenes[idx] = snap;
    else slot.scenes.push(snap);
    this.localShows = libs;
    this.emitShows();
  }

  async recallScene(show: string, name: string, fadeMs?: number): Promise<void> {
    const clean = name.trim();
    try {
      await this.invoke('recall_scene', {
        name: clean,
        fade_ms: fadeMs === undefined ? 400 : Math.max(0, Math.round(fadeMs) || 0),
      });
      return;
    } catch {
      /* shell without scene ops: apply the local fallback instantly */
    }
    const slotName = show.trim() || TAURI_SHOW;
    const scene =
      this.localShows.find((s) => s.name === slotName)?.scenes.find((s) => s.name === clean) ??
      this.localShows.flatMap((s) => s.scenes).find((s) => s.name === clean);
    if (!scene) throw new Error(`unknown scene ${clean}`);
    for (const s of scene.strips) {
      if (!s.muted) await this.setMute(s.ch, false);
    }
    for (const s of scene.strips) await this.setFader(s.ch, s.fader);
    for (const s of scene.strips) {
      if (s.muted) await this.setMute(s.ch, true);
    }
  }

  async deleteScene(show: string, name: string): Promise<void> {
    const clean = name.trim();
    try {
      await this.invoke('delete_scene', { name: clean });
      await this.refreshShows();
      return;
    } catch {
      /* shell without scene ops: drop from the local fallback */
    }
    const slotName = show.trim() || TAURI_SHOW;
    this.localShows = copyShows(this.localShows).map((s) =>
      s.name === slotName ? { ...s, scenes: s.scenes.filter((sc) => sc.name !== clean) } : s,
    );
    this.emitShows();
  }

  // No strip locks in the Tauri shell: claim/release are no-ops, the table
  // stays empty, and no denial ever fires (null-safe for shared UI code).
  async lockStrip(_ch: number): Promise<void> {}
  async unlockStrip(_ch: number): Promise<void> {}

  onLocks(cb: (holds: LockHolds) => void): Unsub {
    try {
      cb({});
    } catch {
      /* ignore */
    }
    return () => {};
  }

  onDeny(_cb: (deny: LockDeny) => void): Unsub {
    return () => {};
  }

  onShows(cb: (shows: ShowLib[]) => void): Unsub {
    const un = sub(this.showsCbs, cb);
    try {
      cb(copyShows(this.localShows));
    } catch {
      /* ignore */
    }
    return un;
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

  onMain(cb: (main: number | null) => void): Unsub {
    const un = sub(this.mainCbs, cb);
    try {
      cb(this.main);
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

  /** Start the ~5 Hz spectrum/GR poll (pull mirror of the WS push). */
  private ensureAvPoll(): void {
    if (this.avTimer !== null) return;
    this.avTimer = window.setInterval(() => {
      if (!this.spectrumCbs.size && !this.grCbs.size) return;
      const chs = this.strips.slice(0, 16).map((s) => s.ch);
      for (const ch of chs) {
        if (this.spectrumCbs.size) {
          void (async () => {
            try {
              const raw = ((await this.invoke('get_spectrum', { ch })) ?? {}) as Record<
                string,
                unknown
              >;
              const binsRaw = raw['bins'];
              if (!Array.isArray(binsRaw)) return;
              const evt: SpectrumEvent = {
                ch,
                bins: binsRaw.map((v: unknown) => Number(v) || 0),
              };
              this.spectrumCbs.forEach((f) => {
                try {
                  f(evt);
                } catch {
                  /* ignore */
                }
              });
            } catch {
              /* backend away: skip frame */
            }
          })();
        }
        if (this.grCbs.size) {
          void (async () => {
            try {
              const raw = ((await this.invoke('get_gr', { ch })) ?? {}) as Record<string, unknown>;
              const evt: GrEvent = {
                ch,
                gate: Number(raw['gate']) || 0,
                comp: Number(raw['comp']) || 0,
              };
              this.grCbs.forEach((f) => {
                try {
                  f(evt);
                } catch {
                  /* ignore */
                }
              });
            } catch {
              /* backend away: skip frame */
            }
          })();
        }
      }
    }, 200);
  }

  // Spectrum frames via the shell's `get_spectrum` pull (~5 Hz poll, same
  // 64-bin shape as the WS `spectrum` tick).
  onSpectrum(cb: (evt: SpectrumEvent) => void): Unsub {
    const un = sub(this.spectrumCbs, cb);
    this.ensureAvPoll();
    return un;
  }

  // GR frames via the shell's `get_gr` pull (~5 Hz poll, same
  // dB-reduction shape as the WS `gr` tick).
  onGr(cb: (evt: GrEvent) => void): Unsub {
    const un = sub(this.grCbs, cb);
    this.ensureAvPoll();
    return un;
  }

  // No param fan-out in the Tauri shell (single client): subscribe works,
  // nothing fires — CALL FOH stays local there.
  onParam(_cb: (evt: ParamEvent) => void): Unsub {
    return () => {};
  }
}

/* ---------------- WebSocket relay (any browser) ---------------- */

export function wsUrlFromQuery(defHost?: string): string {
  const q = new URLSearchParams(location.search);
  const s = (q.get('server') || '').trim();
  if (/^wss?:\/\//i.test(s)) return s;
  if (/^https:\/\//i.test(s)) return `wss://${s.slice(8)}`;
  if (/^http:\/\//i.test(s)) return `ws://${s.slice(7)}`;
  if (s) return `${location.protocol === 'https:' ? 'wss' : 'ws'}://${s}`;
  // Same-origin by default: works on LAN (:8080), behind tunnels and
  // reverse proxies (same host/port/scheme as the page, /ws path).
  // A bare ?server=host[:port] keeps the legacy direct-LAN behavior.
  const pageProto = location.protocol === 'https:' ? 'wss' : 'ws';
  if (defHost) return `${pageProto}://${defHost}:8080`;
  if (!location.hostname) return 'ws://127.0.0.1:8080';
  return `${pageProto}://${location.host}/ws`;
}

export class WsBackend implements Backend {
  readonly kind = 'ws' as const;
  readonly url: string;
  private online = false;
  private readonly stateCbs = new Set<(strips: Strip[]) => void>();
  private readonly meterCbs = new Set<(levels: MeterLevels) => void>();
  private readonly statusCbs = new Set<(online: boolean) => void>();
  private readonly spectrumCbs = new Set<(evt: SpectrumEvent) => void>();
  private readonly grCbs = new Set<(evt: GrEvent) => void>();
  private readonly showsCbs = new Set<(shows: ShowLib[]) => void>();
  private lastShows: ShowLib[] = [];
  private readonly showsWaiters: Array<(shows: ShowLib[]) => void> = [];
  /** LR main fader from the last state frame (null until first state). */
  private lastMain: number | null = null;
  private readonly mainCbs = new Set<(main: number | null) => void>();
  private readonly locksCbs = new Set<(holds: LockHolds) => void>();
  private readonly denyCbs = new Set<(deny: LockDeny) => void>();
  private readonly paramCbs = new Set<(evt: ParamEvent) => void>();
  private readonly clientsCbs = new Set<(info: ClientsInfo) => void>();
  private lastClients: ClientsInfo = { count: 0, writers: [], selfId: null };
  /**
   * Set when we send a mutation the relay echoes with `by`; the next `by`
   * is (barring a concurrent writer) our own relay id. Best-effort: a
   * simultaneous foreign write can misattribute, so this only marks "you".
   */
  private pendingSelf = false;
  private lastHolds: LockHolds = {};
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
    // Mutations the relay echoes with `by` (connect/fader/main_fader/
    // mute/param/recall_scene): arm self-id learning for the echo.
    if (
      obj.cmd === 'connect' ||
      obj.cmd === 'fader' ||
      obj.cmd === 'main_fader' ||
      obj.cmd === 'mute' ||
      obj.cmd === 'param' ||
      obj.cmd === 'recall_scene'
    ) {
      this.pendingSelf = true;
    }
    const msg = JSON.stringify(obj);
    if (this.ws && this.ws.readyState === WebSocket.OPEN) this.ws.send(msg);
    else this.queue.push(msg);
  }

  /** Fallback self-id learning for old servers without the `you` frame:
   * learn our relay id from a `by` echo of our own write, then re-emit.
   * Ignored once the authoritative `you` id is known. */
  private noteBy(by: unknown): void {
    if (typeof by !== 'string' || !by || !this.pendingSelf) return;
    this.pendingSelf = false;
    if (this.lastClients.selfId !== null) return;
    this.lastClients = { ...this.lastClients, selfId: by };
    this.emitClients();
  }

  private emitClients(): void {
    const snap: ClientsInfo = {
      count: this.lastClients.count,
      writers: [...this.lastClients.writers],
      selfId: this.lastClients.selfId,
    };
    this.clientsCbs.forEach((cb) => {
      try {
        cb(snap);
      } catch {
        /* ignore */
      }
    });
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
    if (t === 'you' && typeof rec['id'] === 'string' && rec['id']) {
      // Authoritative self id (unicast on connect + every `connect` cmd):
      // overwrites any echo-guessed value, then re-emits the roster.
      this.lastClients = { ...this.lastClients, selfId: rec['id'] };
      this.emitClients();
      return;
    }
    if (t === 'state' && Array.isArray(rec['strips'])) {
      this.noteBy(rec['by']);
      const strips: Strip[] = rec['strips'].map((s: unknown, i: number) => normStrip(s, i));
      const mf = rec['main_fader'];
      this.lastMain =
        typeof mf === 'number' && Number.isFinite(mf) ? Math.min(1, Math.max(0, mf)) : null;
      const main = this.lastMain;
      this.mainCbs.forEach((cb) => {
        try {
          cb(main);
        } catch {
          /* ignore */
        }
      });
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
    } else if (t === 'gr' && typeof rec['ch'] === 'number') {
      const evt: GrEvent = {
        ch: rec['ch'],
        gate: Number(rec['gate']) || 0,
        comp: Number(rec['comp']) || 0,
      };
      this.grCbs.forEach((cb) => {
        try {
          cb(evt);
        } catch {
          /* ignore */
        }
      });
    } else if (t === 'param' && typeof rec['ch'] === 'number' && typeof rec['field'] === 'string') {
      const raw = rec['value'];
      const value: ParamValue =
        typeof raw === 'string' || typeof raw === 'boolean'
          ? raw
          : typeof raw === 'number' && Number.isFinite(raw)
            ? raw
            : 0;
      const by = typeof rec['by'] === 'string' ? (rec['by'] as string) : undefined;
      this.noteBy(by);
      const evt: ParamEvent = { ch: rec['ch'], field: rec['field'], value, ...(by ? { by } : {}) };
      this.paramCbs.forEach((cb) => {
        try {
          cb(evt);
        } catch {
          /* ignore */
        }
      });
    } else if (t === 'shows' && Array.isArray(rec['shows'])) {
      const shows = normShows(rec['shows']);
      this.lastShows = shows;
      const snap = copyShows(shows);
      this.showsCbs.forEach((cb) => {
        try {
          cb(snap);
        } catch {
          /* ignore */
        }
      });
      for (const w of this.showsWaiters.splice(0)) {
        try {
          w(snap);
        } catch {
          /* ignore */
        }
      }
    } else if (t === 'clients') {
      // Null-safe: old servers may send odd shapes; drop bad entries.
      const countRaw = rec['count'];
      const count =
        typeof countRaw === 'number' && Number.isFinite(countRaw)
          ? Math.max(0, Math.floor(countRaw))
          : this.lastClients.count;
      const writersRaw = rec['writers'];
      const writers = Array.isArray(writersRaw)
        ? writersRaw.filter((w): w is string => typeof w === 'string' && !!w)
        : this.lastClients.writers;
      this.lastClients = { count, writers, selfId: this.lastClients.selfId };
      this.emitClients();
    } else if (t === 'locks' && rec['holds'] !== null && typeof rec['holds'] === 'object') {
      // Null-safe: unknown/old servers may send odd shapes; drop bad keys.
      const holds: LockHolds = {};
      for (const [k, v] of Object.entries(rec['holds'] as Record<string, unknown>)) {
        const ch = Number(k);
        if (Number.isInteger(ch) && ch >= 0 && typeof v === 'string' && v) holds[ch] = v;
      }
      this.lastHolds = holds;
      const snap: LockHolds = { ...holds };
      this.locksCbs.forEach((cb) => {
        try {
          cb(snap);
        } catch {
          /* ignore */
        }
      });
    } else if (t === 'error') {
      const lb = rec['locked_by'];
      const chRaw = rec['ch'];
      const deny: LockDeny = {
        ch: typeof chRaw === 'number' && Number.isFinite(chRaw) ? chRaw : null,
        lockedBy: typeof lb === 'string' && lb ? lb : null,
        message: typeof rec['message'] === 'string' && rec['message'] ? rec['message'] : 'request denied',
      };
      // Only lock denials carry locked_by; generic errors stay silent here
      // (callers already surface their own failures).
      if (deny.lockedBy !== null) {
        this.denyCbs.forEach((cb) => {
          try {
            cb(deny);
          } catch {
            /* ignore */
          }
        });
      }
    } else if (t === 'recalled' && Array.isArray(rec['overrode_locks'])) {
      const chs: number[] = [];
      for (const v of rec['overrode_locks']) {
        if (typeof v === 'number' && Number.isFinite(v)) chs.push(v);
      }
      if (chs.length) {
        const deny: LockDeny = {
          ch: chs[0] ?? null,
          lockedBy: null,
          message: `scene recall overrode lock${chs.length > 1 ? 's' : ''} on ch ${chs.join(', ')}`,
        };
        this.denyCbs.forEach((cb) => {
          try {
            cb(deny);
          } catch {
            /* ignore */
          }
        });
      }
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

  async setMainFader(v01: number): Promise<void> {
    this.send({ cmd: 'main_fader', value: v01 });
  }

  async setMute(ch: number, muted: boolean): Promise<void> {
    this.send({ cmd: 'mute', ch, muted });
  }

  async setParam(ch: number, field: string, v: ParamValue): Promise<void> {
    this.send({ cmd: 'param', ch, field, value: v });
  }

  /**
   * Fresh server library: sends `list_shows` and resolves on the unicast
   * `shows` reply. Rejects on timeout so callers can fall back to cache.
   */
  async listShows(): Promise<ShowLib[]> {
    const next = new Promise<ShowLib[]>((resolve, reject) => {
      let res: (shows: ShowLib[]) => void = () => {};
      const to = window.setTimeout(() => {
        const i = this.showsWaiters.indexOf(res);
        if (i >= 0) this.showsWaiters.splice(i, 1);
        reject(new Error(`shows timeout ${this.url}`));
      }, 5000);
      res = (shows) => {
        window.clearTimeout(to);
        resolve(shows);
      };
      this.showsWaiters.push(res);
    });
    this.send({ cmd: 'list_shows' });
    return next;
  }

  async captureScene(show: string, name: string): Promise<void> {
    this.send({ cmd: 'capture_scene', show, name });
  }

  async recallScene(show: string, name: string, fadeMs?: number): Promise<void> {
    this.send({
      cmd: 'recall_scene',
      show,
      name,
      fade_ms: fadeMs === undefined ? null : Math.max(0, Math.round(fadeMs) || 0),
    });
  }

  async deleteScene(show: string, name: string): Promise<void> {
    this.send({ cmd: 'delete_scene', show, name });
  }

  async lockStrip(ch: number): Promise<void> {
    this.send({ cmd: 'lock', ch });
  }

  async unlockStrip(ch: number): Promise<void> {
    this.send({ cmd: 'unlock', ch });
  }

  onShows(cb: (shows: ShowLib[]) => void): Unsub {
    const un = sub(this.showsCbs, cb);
    try {
      cb(copyShows(this.lastShows));
    } catch {
      /* ignore */
    }
    return un;
  }

  onState(cb: (strips: Strip[]) => void): Unsub {
    return sub(this.stateCbs, cb);
  }
  onMain(cb: (main: number | null) => void): Unsub {
    const un = sub(this.mainCbs, cb);
    try {
      cb(this.lastMain);
    } catch {
      /* ignore */
    }
    return un;
  }
  onMeters(cb: (levels: MeterLevels) => void): Unsub {
    return sub(this.meterCbs, cb);
  }
  onSpectrum(cb: (evt: SpectrumEvent) => void): Unsub {
    return sub(this.spectrumCbs, cb);
  }
  onGr(cb: (evt: GrEvent) => void): Unsub {
    return sub(this.grCbs, cb);
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

  onLocks(cb: (holds: LockHolds) => void): Unsub {
    const un = sub(this.locksCbs, cb);
    try {
      cb({ ...this.lastHolds });
    } catch {
      /* ignore */
    }
    return un;
  }

  onDeny(cb: (deny: LockDeny) => void): Unsub {
    return sub(this.denyCbs, cb);
  }

  onParam(cb: (evt: ParamEvent) => void): Unsub {
    return sub(this.paramCbs, cb);
  }

  onClients(cb: (info: ClientsInfo) => void): Unsub {
    const un = sub(this.clientsCbs, cb);
    try {
      cb({
        count: this.lastClients.count,
        writers: [...this.lastClients.writers],
        selfId: this.lastClients.selfId,
      });
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

// W2a EngineWorld: the World implementation over Transport + protocol + mapping.
//
// Reactive story: plain class, framework-light. Every ingested frame, local
// edit, timeout sweep or status flip bumps `version` and calls subscribers
// (imperatively; no reactive hot paths — SPEC 2a). Meters/GR/taps decode
// straight into preallocated typed arrays (the MeterSim buffer shape:
// level/peak Float32Arrays + clipUntil latches); spectrum keeps one cached
// 64-bin array per streamed channel. The only per-frame garbage is the
// transient decode array from parseBinary (bounded, small, copied out).
//
// Param writes are optimistic local-first (proven logic adapted from
// v3/state/paramLogic.ts, q.v.): leading-edge send, then latest-wins
// coalescing at <= 60/s per control (writeCoalescer.ts); confirmation via
// `param`/`params` (or `state` for fader/mute) clears pending; unconfirmed
// edits revert after CONFIRM_TIMEOUT_MS (see sweep()).

import type {
  CapsRow as WorldCapsRow,
  DenyInfo,
  DeviceView,
  DynView,
  EqView,
  FxSlotView,
  InfoView,
  InputView,
  LinkView,
  MeterState,
  OfflineChangeView,
  PatchView,
  ParamStatus,
  ParamValue,
  SceneEntry,
  SendView,
  ShowLib,
  StripKind,
  Unsub,
  World,
  WorldBank,
  WorldStrip,
} from '../types.ts';
import {
  cmdConnect,
  cmdFader,
  cmdFocus,
  cmdLock,
  cmdMainFader,
  cmdMute,
  cmdParam,
  cmdUnlock,
  parseBinary,
  parseEvent,
  type EngineEvent,
  type TopologyEvt,
} from '../protocol.ts';
import type { Transport, TransportMessage } from '../transport.ts';
import { deriveBanks } from '../mapping/banks.ts';
import { lawForDriver } from '../mapping/faderLaw.ts';
import { COLOR_HEX, clamp, enginePanToDesign } from '../mapping/params.ts';
import {
  CONFIRM_TIMEOUT_MS,
  FOCUS_DEBOUNCE_MS,
  applyParams,
  capsFromInput,
  coerceWrite,
  confirmedKnown,
  emptyCaps,
  emptyMaps,
  isPending,
  normalizeFocus,
  noteOptimistic,
  optimisticValue,
  sameList,
  splitKey,
  sweepPending,
  valuesEqual,
  type CapsSnapshot,
  type StoreMaps,
} from './paramStore.ts';
import { WriteCoalescer } from './writeCoalescer.ts';
import {
  buildChannelDyn,
  buildChannelEq,
  buildChannelInput,
  buildChannelSends,
  writeEqBandType as writeEqBandTypeTo,
  writePan as writePanTo,
  writeSendLevel as writeSendLevelTo,
  writeSendPre as writeSendPreTo,
  writeSlope as writeSlopeTo,
} from './channelViews.ts';
import { LibraryStore } from './libraries.ts';
export interface EngineWorldOptions {
  now?: () => number;
  perfNow?: () => number;
  setTimer?: (fn: () => void, ms: number) => unknown;
  clearTimer?: (t: unknown) => void;
  confirmTimeoutMs?: number;
  focusDebounceMs?: number;
}

interface StateStrip {
  name: string;
  fader: number;
  muted: boolean;
}

/** Topology identity for change detection: driver switches and family
 * relayouts must invalidate per-strip state (see the `caps` handler);
 * rebroadcasts of the same layout must not. */
function topoFingerprint(topo: readonly TopologyEvt[]): string {
  return topo.map((f) => `${f.kind}:${f.first}:${f.count}`).join(',');
}

export class EngineWorld implements World {
  readonly kind = 'engine';
  version = 0;

  private readonly transport: Transport;
  private readonly clock: () => number;
  private readonly perf: () => number;
  private readonly setTimer: (fn: () => void, ms: number) => unknown;
  private readonly clearTimer: (t: unknown) => void;
  private readonly confirmTimeoutMs: number;
  private readonly focusDebounceMs: number;
  private readonly coalescer: WriteCoalescer;

  private unsubs: Unsub[] = [];
  private readonly subs = new Set<() => void>();
  private readonly statusSubs = new Set<(online: boolean) => void>();

  private _online = false;
  private _driver = '';
  private _selfId: string | null = null;
  private caps: CapsSnapshot = emptyCaps();
  private topology: TopologyEvt[] = [];
  private _banks: WorldBank[] = [];
  private maps: StoreMaps = emptyMaps();
  private states = new Map<number, StateStrip>();
  private _mainFader: number | null = null;
  private mainPending: { value: number; at: number } | null = null;
  private locks = new Map<number, string>();
  private _clients = { count: 0, writers: [] as string[], selfId: null as string | null };
  private _lastDeny: DenyInfo | null = null;
  /** Shows/scenes/FX/routing/devices/link/info/offline (libraries.ts). */
  private readonly lib: LibraryStore;

  // meters (preallocated; resized on topology/state/meters growth)
  private _stripCount = 0;
  private level = new Float32Array(0);
  private peak = new Float32Array(0);
  private peakUntil = new Float64Array(0);
  private clipUntil = new Float64Array(0);
  private grGate = new Float32Array(0);
  private grComp = new Float32Array(0);
  private spectra = new Map<number, Float32Array>();
  private _taps: Record<string, Float32Array> = {};

  private _connectedAt: number | null = null;
  private firstSeenAt = new Map<string, number>();

  private visible: number[] = [];
  private openChannel: number | null = null;
  private focusTimer: unknown = null;
  private lastFocusSent: number[] = [];

  private readonly meterApi: MeterState;

  constructor(transport: Transport, opts: EngineWorldOptions = {}) {
    this.transport = transport;
    this.clock = opts.now ?? (() => Date.now());
    this.perf =
      opts.perfNow ??
      (() => (typeof performance !== 'undefined' ? performance.now() : Date.now()));
    this.setTimer = opts.setTimer ?? ((fn, ms) => setTimeout(fn, ms));
    this.clearTimer = opts.clearTimer ?? ((t) => clearTimeout(t as ReturnType<typeof setTimeout>));
    this.confirmTimeoutMs = opts.confirmTimeoutMs ?? CONFIRM_TIMEOUT_MS;
    this.focusDebounceMs = opts.focusDebounceMs ?? FOCUS_DEBOUNCE_MS;
    this.coalescer = new WriteCoalescer({
      now: this.clock,
      setTimer: this.setTimer,
      clearTimer: this.clearTimer,
    });
    this.meterApi = {
      size: 0,
      level: this.level,
      peak: this.peak,
      clipUntil: this.clipUntil,
      grGate: this.grGate,
      grComp: this.grComp,
      spectrum: (ch: number) => this.spectra.get(ch) ?? null,
      taps: this._taps,
    };
    this.lib = new LibraryStore((msg) => this.transport.send(msg));
    this.unsubs.push(transport.onMessage((m) => this.ingest(m)));
    this.unsubs.push(
      transport.onStatus((online) => {
        this._online = online;
        if (online && this._connectedAt === null) this._connectedAt = this.perf();
        this.bump();
        this.statusSubs.forEach((cb) => {
          try {
            cb(online);
          } catch {
            /* ignore */
          }
        });
      }),
    );
  }

  destroy(): void {
    for (const u of this.unsubs.splice(0)) {
      try {
        u();
      } catch {
        /* ignore */
      }
    }
    this.subs.clear();
    this.statusSubs.clear();
    this.coalescer.clear();
    if (this.focusTimer !== null) {
      this.clearTimer(this.focusTimer);
      this.focusTimer = null;
    }
  }

  private bump(): void {
    this.version++;
    this.subs.forEach((cb) => {
      try {
        cb();
      } catch {
        /* ignore */
      }
    });
  }

  // ---- connection ----

  get online(): boolean {
    return this._online;
  }
  get driver(): string {
    return this._driver || this.caps.driver;
  }
  get selfId(): string | null {
    return this._selfId;
  }
  get connectedAt(): number | null {
    return this._connectedAt;
  }
  get pendingCount(): number {
    return Object.keys(this.maps.pending).length + (this.mainPending ? 1 : 0);
  }

  connect(driver?: string, addr?: string): void {
    this.maps = emptyMaps();
    this.firstSeenAt.clear();
    this.mainPending = null;
    this._connectedAt = this.perf();
    this.transport.connect();
    // Attach-only when the caller chose no driver: open the socket (the
    // transport sends hello on open) and accept whatever driver the relay
    // already runs — the relay's hello/state reports it. A
    // driver-switching Connect goes out only when a driver was explicitly
    // chosen (connection screen), never on auto-attach: an unsolicited
    // Connect flips a hardware-desk relay off its desk driver.
    if (driver !== undefined) this.transport.send(cmdConnect(driver, addr || undefined));
    this.bump();
  }

  onStatus(cb: (online: boolean) => void): Unsub {
    this.statusSubs.add(cb);
    try {
      cb(this._online);
    } catch {
      /* ignore */
    }
    return () => {
      this.statusSubs.delete(cb);
    };
  }

  subscribe(cb: () => void): Unsub {
    this.subs.add(cb);
    return () => {
      this.subs.delete(cb);
    };
  }

  // ---- ingest ----

  private ingest(msg: TransportMessage): void {
    if (typeof msg === 'string') {
      const evt = parseEvent(msg);
      if (!evt) return;
      // SPEC 2a: meter-class frames ride the preallocated typed arrays and
      // never poke reactive subscribers (no per-frame reactive hot path);
      // canvas readers repaint on their own cadence.
      const meterClass =
        evt.kind === 'meters' || evt.kind === 'spectrum' || evt.kind === 'gr' || evt.kind === 'taps';
      if (evt.kind === 'unknown') return; // newer-engine frames never disturb the UI
      this.apply(evt);
      if (!meterClass) this.bump();
      return;
    }
    const bin = parseBinary(msg);
    if (!bin) return;
    if (bin.kind === 'meters') this.applyLevels(bin.levels);
    else if (bin.kind === 'gr') this.applyGr(bin.ch, bin.gate, bin.comp);
    else this.applyTap(bin.tap, bin.levels);
  }

  private apply(evt: EngineEvent): void {
    if (evt.kind === 'info' && evt.driver) this._driver = evt.driver;
    if (this.lib.apply(evt)) return;
    switch (evt.kind) {
      case 'unknown':
        return;
      case 'you':
        this._selfId = evt.id;
        this._clients = { ...this._clients, selfId: evt.id };
        break;
      case 'state':
        this._driver = evt.driver || this._driver;
        this.ensureStripCount(evt.strips.reduce((m, s) => Math.max(m, s.ch + 1), 0));
        for (const s of evt.strips) {
          this.states.set(s.ch, { name: s.name, fader: s.fader, muted: s.muted });
          // The state frame carries the confirmed fader/mute/name: clear matching pending.
          const slot = this.maps.confirmed[s.ch] ?? (this.maps.confirmed[s.ch] = {});
          if (slot['fader'] !== s.fader) {
            slot['fader'] = s.fader;
            this.markSeen(s.ch, 'fader');
          }
          if (slot['mute'] !== s.muted) slot['mute'] = s.muted;
          if (typeof slot['name'] !== 'string' || slot['name'] !== s.name) slot['name'] = s.name;
          for (const id of ['fader', 'mute', 'name'] as const) {
            const pend = this.maps.pending[`${s.ch}:${id}`];
            if (pend) {
              const cur = id === 'fader' ? s.fader : id === 'mute' ? s.muted : s.name;
              // Desk-quantized fader echoes (cf. valuesEqual) must clear the
              // edit like params frames do; strict equality strands it.
              // Mute/name still compare exactly via valuesEqual's fallback.
              if (valuesEqual(pend.value, cur)) delete this.maps.pending[`${s.ch}:${id}`];
            }
          }
        }
        if (evt.main_fader !== null) {
          this._mainFader = evt.main_fader;
          // Same desk-quantization agreement as valuesEqual (cf. GAP A):
          // a quantized echo must clear the optimistic MAIN edit.
          if (this.mainPending && valuesEqual(this.mainPending.value, evt.main_fader)) {
            this.mainPending = null;
          }
        }
        break;
      case 'param':
        if (applyParams(this.maps, evt.ch, { [evt.field]: evt.value })) this.markSeen(evt.ch, evt.field);
        else if (confirmedKnown(this.maps, evt.ch, evt.field)) this.markSeen(evt.ch, evt.field);
        this.ensureStripCount(evt.ch + 1);
        break;
      case 'params':
        for (const id of Object.keys(evt.values)) {
          if (!confirmedKnown(this.maps, evt.ch, id)) this.markSeen(evt.ch, id);
        }
        applyParams(this.maps, evt.ch, evt.values);
        this.ensureStripCount(evt.ch + 1);
        break;
      case 'caps': {
        this.caps = capsFromInput({ driver: evt.driver, params: evt.params, unsupported: evt.unsupported });
        // Same-socket driver switch (sim -> xair via ConnectDevice) swaps
        // the topology under this world while per-strip confirmed/pending
        // values persist: raw strip 30 is sim in31 ("CH31") but xair Main
        // LR, so the stale sim values would reinterpret as the master
        // (and the empty xair LR scribble never heals the name). Drop all
        // per-strip -- and per-strip-timed -- state when the driver or the
        // family layout actually changed. Pure caps rebroadcasts (param
        // drops) keep an identical topology and must NOT wipe pending edits.
        const nextDriver = evt.driver || this._driver;
        // First attach carries state before caps (empty topology): that is
        // fresh truth, not a switch -- only invalidate once a layout was
        // actually established.
        if (
          this.topology.length > 0 &&
          (nextDriver !== this._driver || topoFingerprint(this.topology) !== topoFingerprint(evt.topology))
        ) {
          this.maps = emptyMaps();
          this.states.clear();
          this.firstSeenAt.clear();
          this.mainPending = null;
        }
        this._driver = nextDriver;
        this.topology = evt.topology;
        this._banks = deriveBanks(this.topology);
        this.lib.setCaps(
          { slots: evt.fx.slots, types: evt.fx.types.map((t) => ({ id: t.id, label: t.label, params: t.params.map((q) => ({ id: q.id, label: q.label, unit: (q as { unit?: string }).unit })) })) },
          evt.routing.supported,
          evt.groups,
          evt.files.supported,
        );
        const end = this.topology.reduce((m, f) => Math.max(m, f.first + f.count), 0);
        this.ensureStripCount(end);
        break;
      }
      case 'meters':
        this.applyLevels(evt.levels);
        break;
      case 'spectrum': {
        let arr = this.spectra.get(evt.ch);
        if (!arr) {
          arr = new Float32Array(64);
          this.spectra.set(evt.ch, arr);
        }
        const n = Math.min(64, evt.bins.length);
        for (let i = 0; i < n; i++) arr[i] = evt.bins[i]!;
        break;
      }
      case 'gr':
        this.applyGr(evt.ch, evt.gate, evt.comp);
        break;
      case 'taps':
        this.applyTap(evt.tap, evt.levels);
        break;
      case 'locks':
        this.locks = new Map(
          Object.entries(evt.holds)
            .map(([k, v]): [number, string] => [Number(k), v])
            .filter(([k, v]) => Number.isInteger(k) && (k as number) >= 0 && !!v),
        );
        break;
      case 'clients':
        this._clients = { count: evt.count, writers: evt.writers.slice(), selfId: this._selfId };
        break;
      case 'error':
        this._lastDeny = {
          ch: evt.ch ?? null,
          lockedBy: evt.locked_by ?? null,
          message: evt.message,
        };
        break;
      case 'recalled':
        if (evt.overrode_locks.length) {
          this._lastDeny = {
            ch: evt.overrode_locks[0] ?? null,
            lockedBy: null,
            message: `scene recall overrode locks on ch ${evt.overrode_locks.join(', ')}`,
          };
        }
        break;
      case 'hello':
        break;
      case 'stats':
        (this as { lastStats?: EngineEvent }).lastStats = evt;
        break;
    }
    // NOTE: no bump here — ingest() bumps for control frames only, so the
    // 20 Hz meter fan-out (meters + 16× gr + 16× spectrum) never touches
    // reactive subscribers (SPEC 2a). Callers that mutate outside ingest
    // (setParam etc.) bump themselves.
  }

  private markSeen(ch: number, id: string): void {
    const key = `${ch}:${id}`;
    if (!this.firstSeenAt.has(key)) this.firstSeenAt.set(key, this.perf());
  }

  firstValueAt(ch: number, id: string): number | null {
    return this.firstSeenAt.get(`${ch}:${id}`) ?? null;
  }

  private ensureStripCount(n: number): void {
    if (n <= this._stripCount) return;
    const next = n;
    const grow = (a: Float32Array<ArrayBuffer>): Float32Array<ArrayBuffer> => {
      const b = new Float32Array(next);
      b.set(a.subarray(0, Math.min(a.length, next)));
      return b;
    };
    const growF = (a: Float64Array<ArrayBuffer>): Float64Array<ArrayBuffer> => {
      const b = new Float64Array(next);
      b.set(a.subarray(0, Math.min(a.length, next)));
      return b;
    };
    this.level = grow(this.level);
    this.peak = grow(this.peak);
    this.grGate = grow(this.grGate);
    this.grComp = grow(this.grComp);
    this.peakUntil = growF(this.peakUntil);
    this.clipUntil = growF(this.clipUntil);
    for (const k of Object.keys(this._taps)) {
      const b = new Float32Array(next);
      b.set(this._taps[k]!.subarray(0, Math.min(this._taps[k]!.length, next)));
      this._taps[k] = b;
    }
    this._stripCount = next;
    this.meterApi.size = next;
    this.meterApi.level = this.level;
    this.meterApi.peak = this.peak;
    this.meterApi.clipUntil = this.clipUntil;
    this.meterApi.grGate = this.grGate;
    this.meterApi.grComp = this.grComp;
  }

  private applyLevels(levels: readonly number[]): void {
    this.ensureStripCount(levels.length);
    const now = this.perf();
    for (let i = 0; i < levels.length; i++) {
      const v = clamp(levels[i] ?? 0, 0, 1);
      this.level[i] = v;
      if (v >= this.peak[i]! || now > this.peakUntil[i]!) {
        this.peak[i] = v;
        this.peakUntil[i] = now + 1500;
      }
      if (v > 0.966) this.clipUntil[i] = now + 1500; // ~-0.3 dBFS, cf. meterSim
    }
  }

  private applyGr(ch: number, gate: number, comp: number): void {
    this.ensureStripCount(ch + 1);
    this.grGate[ch] = Math.max(0, gate);
    this.grComp[ch] = Math.max(0, comp);
  }

  private applyTap(tap: string, levels: readonly number[]): void {
    this.ensureStripCount(levels.length);
    let arr = this._taps[tap];
    if (!arr) {
      arr = new Float32Array(this._stripCount);
      this._taps[tap] = arr;
    }
    for (let i = 0; i < levels.length; i++) arr[i] = levels[i] ?? 0;
  }

  // ---- topology / banks / strips ----

  get stripCount(): number {
    return this._stripCount;
  }
  get banks(): WorldBank[] {
    return this._banks.map((b) => ({ ...b, strips: b.strips.slice(), families: b.families.slice() }));
  }
  get meters(): MeterState {
    return this.meterApi;
  }

  private familyOf(ch: number): TopologyEvt | null {
    for (const f of this.topology) {
      if (ch >= f.first && ch < f.first + f.count) return f;
    }
    return null;
  }

  kindOf(ch: number): StripKind | null {
    const f = this.familyOf(ch);
    if (!f) return null;
    return (['in', 'ret', 'bus', 'fxsnd', 'mtx', 'main', 'dca'] as StripKind[]).includes(f.kind as StripKind)
      ? (f.kind as StripKind)
      : null;
  }

  bankFor(ch: number): WorldBank | null {
    const b = this._banks.find((x) => x.strips.includes(ch));
    return b ? { ...b, strips: b.strips.slice(), families: b.families.slice() } : null;
  }

  strip(ch: number): WorldStrip {
    const fam = this.familyOf(ch);
    const kind: StripKind = this.kindOf(ch) ?? 'in';
    const local = fam ? ch - fam.first : ch;
    const id = fam ? fam.key + (local + 1) : 'ch' + (ch + 1);
    const confirmed = this.maps.confirmed[ch];
    const state = this.states.get(ch);
    const nameV = confirmed?.['name'];
    const name =
      typeof nameV === 'string' && nameV
        ? nameV
        : (state?.name ?? '') || (fam ? fam.name_prefix + ' ' + (local + 1) : 'Ch ' + (ch + 1));
    const srcV = confirmed?.['source'];
    const source =
      typeof srcV === 'string' && srcV
        ? srcV
        : fam
          ? fam.name_prefix + ' ' + String(local + 1).padStart(2, '0')
          : '';
    const colorV = confirmed?.['color'];
    const colorIdx =
      typeof colorV === 'number' && Number.isFinite(colorV) ? Math.round(colorV) : (fam?.color_default ?? 2);
    const colour = COLOR_HEX[Math.min(COLOR_HEX.length - 1, Math.max(0, colorIdx))] ?? '#64748b';
    const bank = this.bankFor(ch);
    const text = bank && bank.colour === colour ? bank.text : contrastText(colour);
    const faderV = optimisticValue(this.maps, ch, 'fader');
    const fader = typeof faderV === 'number' ? faderV : (state?.fader ?? 0);
    const muteV = optimisticValue(this.maps, ch, 'mute');
    const muted = typeof muteV === 'boolean' ? muteV : (state?.muted ?? false);
    const panField = this.supports('balance', ch) ? 'balance' : 'pan';
    const panV = optimisticValue(this.maps, ch, panField);
    const pan = this.supports(panField, ch) && typeof panV === 'number' ? enginePanToDesign(panV) : null;
    const lockedBy = this.locks.get(ch) ?? null;
    return { ch, id, name, source, colour, text, kind, fader, muted, pan, locked: lockedBy !== null, lockedBy };
  }

  get mainFader(): number | null {
    if (this.mainPending) return this.mainPending.value;
    return this.lib.mainMaster.fader ?? this._mainFader;
  }

  /** Main LR master mute (pending-optimistic not tracked: the mute button
   *  flips on echo; null = unknown yet). */
  get mainMuted(): boolean | null {
    return this.lib.mainMaster.muted;
  }

  // ---- params / caps ----

  get capsReady(): boolean {
    return this.caps.ready;
  }
  get unsupported(): string[] {
    return this.caps.unsupported.slice();
  }

  def(id: string): WorldCapsRow | null {
    const d = this.caps.defs[id];
    return d ? { ...d, kind: d.kind as WorldCapsRow['kind'], scopes: d.scopes.slice() } : null;
  }

  supports(id: string, ch?: number): boolean {
    if (!this.caps.ready) return false;
    const d = this.caps.defs[id];
    if (!d || this.caps.unsupported.includes(id)) return false;
    if (ch === undefined) return true;
    if (d.scopes.length) {
      const kind = this.kindOf(ch);
      if (!kind) return false;
      return d.scopes.includes(kind);
    }
    // Legacy caps without scopes: coarse single-scope tag.
    if (d.kind && this.kindOf(ch) === null && this.topology.length) return false;
    return true;
  }

  param(ch: number, id: string): ParamValue | null {
    return optimisticValue(this.maps, ch, id);
  }

  status(ch: number, id: string): ParamStatus {
    if (this.caps.ready) {
      const d = this.caps.defs[id];
      if (!d || this.caps.unsupported.includes(id)) return 'unsupported';
      if (d.scopes.length) {
        const kind = this.kindOf(ch);
        if (kind && !d.scopes.includes(kind)) return 'unsupported';
      }
    }
    if (isPending(this.maps, ch, id)) return 'pending';
    if (confirmedKnown(this.maps, ch, id)) return 'known';
    return 'unknown';
  }

  private sendParam(ch: number, id: string, v: ParamValue): void {
    this.transport.send(cmdParam(ch, id, v));
  }

  setParam(ch: number, id: string, v: ParamValue): boolean {
    if (!this.supports(id, ch)) return false;
    const coerced = coerceWrite(this.caps.defs[id], v);
    if (coerced === null) return false;
    const key = `${ch}:${id}`;
    const now = this.clock();
    const res = this.coalescer.offer(key, () => this.sendParam(ch, id, coerced));
    void res;
    noteOptimistic(this.maps, ch, id, coerced, now);
    this.bump();
    return true;
  }

  setFader(ch: number, v01: number): boolean {
    if (!this.supports('fader', ch)) return false;
    if (typeof v01 !== 'number' || !Number.isFinite(v01)) return false;
    const v = clamp(v01, 0, 1);
    const key = `${ch}:fader`;
    const now = this.clock();
    this.coalescer.offer(key, () => this.transport.send(cmdFader(ch, v)));
    noteOptimistic(this.maps, ch, 'fader', v, now);
    this.bump();
    return true;
  }

  setMute(ch: number, muted: boolean): boolean {
    if (!this.supports('mute', ch)) return false;
    if (typeof muted !== 'boolean') return false;
    const now = this.clock();
    this.coalescer.offer(`${ch}:mute`, () => this.transport.send(cmdMute(ch, muted)));
    noteOptimistic(this.maps, ch, 'mute', muted, now);
    this.bump();
    return true;
  }

  setMainFader(v01: number): boolean {
    if (!this.caps.ready || !this.caps.defs['fader'] || this.caps.unsupported.includes('fader')) return false;
    if (typeof v01 !== 'number' || !Number.isFinite(v01)) return false;
    const v = clamp(v01, 0, 1);
    this.coalescer.offer('main:fader', () => this.transport.send(cmdMainFader(v)));
    this.mainPending = { value: v, at: this.clock() };
    this.bump();
    return true;
  }

  focus(chs: number[]): void {
    const list = normalizeFocus(chs);
    if (!list.length) return;
    this.transport.send(cmdFocus(list));
  }

  setVisible(chs: number[]): void {
    this.visible = normalizeFocus(chs);
    this.scheduleFocus();
  }

  setOpenChannel(ch: number | null): void {
    this.openChannel = ch !== null && Number.isInteger(ch) && ch >= 0 ? ch : null;
    this.scheduleFocus();
  }

  private scheduleFocus(): void {
    if (this.focusTimer !== null) this.clearTimer(this.focusTimer);
    this.focusTimer = this.setTimer(() => {
      this.focusTimer = null;
      // MRU first: open channel, then visible, deduped.
      const ordered = normalizeFocus([
        ...(this.openChannel !== null ? [this.openChannel] : []),
        ...this.visible,
      ]);
      if (!ordered.length || sameList(ordered, this.lastFocusSent)) return;
      this.lastFocusSent = ordered;
      this.transport.send(cmdFocus(ordered));
    }, this.focusDebounceMs);
  }

  sweep(nowMs: number): string[] {
    const expired = sweepPending(this.maps.pending, nowMs, this.confirmTimeoutMs);
    if (this.mainPending && nowMs - this.mainPending.at >= this.confirmTimeoutMs) this.mainPending = null;
    if (expired.length) this.bump();
    return expired
      .map((k) => splitKey(k))
      .filter((s): s is { ch: number; id: string } => s !== null)
      .map((s) => `${s.ch}:${s.id}`);
  }

  // ---- channel detail views (design model over engine ids; builders live
  // in channelViews.ts, one concern per file) ----

  /** Confirmed-value probe for the view builders (band-5 presence). */
  confirmed(ch: number, id: string): boolean {
    return confirmedKnown(this.maps, ch, id);
  }

  channelEq(ch: number): EqView {
    return buildChannelEq(this, ch);
  }

  channelDyn(ch: number): DynView {
    return buildChannelDyn(this, ch);
  }

  channelSends(ch: number): SendView[] {
    return buildChannelSends(this, ch);
  }

  channelInput(ch: number): InputView {
    return buildChannelInput(this, ch);
  }

  // ---- fader law ----

  get lawId(): string {
    return lawForDriver(this.driver).id;
  }
  faderToDb(f: number): number {
    return lawForDriver(this.driver).toDb(f);
  }
  dbToFader(db: number): number {
    return lawForDriver(this.driver).fromDb(db);
  }

  /** Design-side write helpers (used by W1 binding; tested round-trip). */
  writeEqBandType(ch: number, band: number, t: 'LS' | 'PEQ' | 'HS'): boolean {
    return writeEqBandTypeTo(this, ch, band, t);
  }
  writeSlope(ch: number, s: 12 | 24): boolean {
    return writeSlopeTo(this, ch, s);
  }
  writePan(ch: number, p: number): boolean {
    return writePanTo(this, ch, p);
  }
  writeSendLevel(ch: number, n: number, v01: number): boolean {
    return writeSendLevelTo(this, ch, n, v01);
  }
  writeSendPre(ch: number, n: number, pre: boolean): boolean {
    return writeSendPreTo(this, ch, n, pre);
  }

  // ---- locks / clients ----

  lockedBy(ch: number): string | null {
    return this.locks.get(ch) ?? null;
  }
  lockStrip(ch: number): void {
    this.transport.send(cmdLock(ch));
  }
  unlockStrip(ch: number): void {
    this.transport.send(cmdUnlock(ch));
  }
  get clients(): { count: number; writers: string[]; selfId: string | null } {
    return { count: this._clients.count, writers: this._clients.writers.slice(), selfId: this._selfId };
  }
  get lastDeny(): DenyInfo | null {
    return this._lastDeny ? { ...this._lastDeny } : null;
  }

  // ---- shows / scenes (state + commands live in libraries.ts) ----

  get shows(): ShowLib[] {
    return this.lib.shows;
  }
  get numberedScenes(): SceneEntry[] {
    return this.lib.numberedScenes;
  }
  listShows(): void {
    this.lib.listShows();
  }
  captureScene(show: string, name: string): void {
    this.lib.captureScene(show, name);
  }
  recallScene(show: string, name: string, fadeMs?: number): void {
    this.lib.recallScene(show, name, fadeMs);
  }
  deleteScene(show: string, name: string): void {
    this.lib.deleteScene(show, name);
  }
  sceneSave(n: number, name: string): void {
    this.lib.sceneSave(n, name);
  }
  sceneRecall(n: number, fadeMs?: number, scope?: { strips?: number[]; groups?: string[]; globals?: string[] } | null): void {
    this.lib.sceneRecall(n, fadeMs, scope ?? null);
    this.bump();
  }
  sceneRename(n: number, name: string): void {
    this.lib.sceneRename(n, name);
  }
  sceneDelete(n: number): void {
    this.lib.sceneDelete(n);
  }
  sceneClear(): void {
    this.lib.sceneClear();
  }

  // ---- fx ----

  get fxSlots(): FxSlotView[] {
    return this.lib.fxSlots;
  }
  get fxCaps(): { slots: number; types: Array<{ id: string; label: string; params: Array<{ id: string; label: string; unit?: string }> }> } | null {
    return this.lib.fxCaps;
  }
  fxSetType(slot: number, type: string): void {
    this.lib.fxSetType(slot, type);
  }
  fxParam(slot: number, id: string, v01: number): void {
    this.lib.fxParam(slot, id, v01);
  }
  fxBypass(slot: number, bypass: boolean): void {
    this.lib.fxBypass(slot, bypass);
  }
  fxOn(slot: number, on: boolean): void {
    this.lib.fxOn(slot, on);
  }

  // ---- routing ----

  get patches(): PatchView[] {
    return this.lib.patches;
  }
  get routingSupported(): boolean {
    return this.lib.routingSupported;
  }
  patchSet(strip: number, group: string, index: number): void {
    this.lib.patchSet(strip, group, index);
  }
  patchClear(strip: number): void {
    this.lib.patchClear(strip);
  }

  // ---- devices / link / info / offline ----

  get devices(): DeviceView[] {
    return this.lib.devices;
  }
  get scannedAt(): number {
    return this.lib.scannedAt;
  }
  scan(): void {
    this.lib.scan();
  }
  probeDesk(addr: string): void {
    this.lib.probeDesk(addr);
  }
  connectDevice(id: string): void {
    this.lib.connectDevice(id);
  }
  disconnectDesk(): void {
    this.lib.disconnectDesk();
  }
  requestInfo(): void {
    this.lib.requestInfo();
  }
  get link(): LinkView | null {
    return this.lib.link;
  }
  get info(): InfoView | null {
    return this.lib.info;
  }
  get offlineDiff(): OfflineChangeView[] {
    return this.lib.offlineDiff;
  }
  get offlineCount(): number {
    return this.lib.offlineCount;
  }
  offline(model: string): void {
    this.lib.offline(model);
  }
  offlinePush(confirm: boolean): void {
    this.lib.offlinePush(confirm);
  }
  offlineDiscard(): void {
    this.lib.offlineDiscard();
  }

  // ---- masters / groups / files (state + commands live in libraries.ts) ----

  get groupCaps(): { buses: number; subgroups: number; muteGroups: number; mgAssign: boolean; subAssign: boolean; files: boolean } {
    return this.lib.groupCaps;
  }
  busMaster(bus: number): { fader: number; muted: boolean } | null {
    return this.lib.busMaster(bus);
  }
  subgroupMaster(group: number): { fader: number; muted: boolean } | null {
    return this.lib.subgroupMaster(group);
  }
  muteGroupMaster(group: number): boolean | null {
    return this.lib.muteGroupMaster(group);
  }
  subgroupAssigned(ch: number, group: number): boolean | null {
    return this.lib.subgroupAssigned(ch, group);
  }
  muteGroupAssigned(ch: number, group: number): boolean | null {
    return this.lib.muteGroupAssigned(ch, group);
  }
  get lastFile(): { filename: string; size: number; fileType: number } | null {
    return this.lib.lastFile;
  }
  busFader(bus: number, value: number): void {
    this.lib.busFader(bus, value);
  }
  busMute(bus: number, muted: boolean): void {
    this.lib.busMute(bus, muted);
  }
  subgroupFader(group: number, value: number): void {
    this.lib.subgroupFader(group, value);
  }
  subgroupMute(group: number, muted: boolean): void {
    this.lib.subgroupMute(group, muted);
  }
  subgroupAssign(ch: number, group: number, member: boolean): void {
    this.lib.subgroupAssign(ch, group, member);
  }
  muteGroup(group: number, muted: boolean): void {
    this.lib.muteGroup(group, muted);
  }
  muteGroupAssign(ch: number, group: number, member: boolean): void {
    this.lib.muteGroupAssign(ch, group, member);
  }
  /** Main LR master state (null fields = unknown yet). */
  get mainMaster(): { fader: number | null; muted: boolean | null } {
    return this.lib.mainMaster;
  }
  /** Per-strip solo (false when never echoed). */
  soloOf(ch: number): boolean {
    return this.lib.soloOf(ch);
  }
  setMainMute(muted: boolean): void {
    if (typeof muted !== 'boolean') return;
    this.lib.setMainMute(muted);
    this.bump();
  }
  setSolo(ch: number, soloed: boolean): void {
    if (!Number.isInteger(ch) || ch < 0 || typeof soloed !== 'boolean') return;
    this.lib.setSolo(ch, soloed);
    this.bump();
  }
  /** Desk fire-and-forget clear-solo (monitor bus only; no echo to wait for). */
  clearSolo(): void {
    this.lib.clearSolo();
    this.bump();
  }
  fileDownload(filename: string): void {
    this.lib.fileDownload(filename);
  }
  fileUpload(filename: string, data: string, fileType: number): void {
    this.lib.fileUpload(filename, data, fileType);
  }

}

/** Contrast text for a name-bar hex (light bg -> dark ink). */
function contrastText(hex: string): string {
  const m = /^#([0-9a-f]{6})$/i.exec(hex);
  if (!m) return '#ffffff';
  const n = parseInt(m[1]!, 16);
  const r = (n >> 16) & 255;
  const g = (n >> 8) & 255;
  const b = n & 255;
  const lum = (0.299 * r + 0.587 * g + 0.114 * b) / 255;
  return lum > 0.6 ? '#10240f' : '#ffffff';
}

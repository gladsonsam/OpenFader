// W2a World interface: the DATA + COMMAND seam the design screens need.
//
// Read from what the screens/store actually consume (ui/state/uiStore.svelte.ts,
// ui/state/uiLogic.ts, ui/fixtures/designData.ts, ui/screens/* view-models):
// strips by bank, per-strip name/colour/source, the DESIGN channel model per
// strip (fader 0..1 + dB text, mute, pan -1..1, eq {on,hpf,bands[4]},
// dyn {gate,comp}, sends[16] + pre/post, input {gain,trim,phantom,invert,
// delay}), per-param status, caps ranges, meter buffers, scenes, fx, routing,
// devices/link/info, offline, focus.
//
// Backing protocol elements (crates/engine = engine, adapters move Cmd/Event
// JSON; see crates/engine/README.md + src/protocol.rs):
//   strips .......... `state` frame (name/fader/muted) + `params` name/source/color
//   params .......... `caps` (ranges/kinds/scopes) + `params`/`param` (values)
//   focus ........... `focus` command (poller priority; SPEC poll budget)
//   meters .......... JSON `meters`/`gr`/`spectrum` or WS BINARY 1/2, taps via
//                     `hello {taps}` -> JSON `taps` or BINARY 3
//   scenes .......... legacy `shows` + R2 numbered `scenes`
//   fx .............. R2 `fx` + caps.fx
//   routing ......... R2 `routing` + caps.routing
//   devices/link .... R2 `devices`/`link`/`info` (+ `scan`/`connect_device`)
//   offline ......... R2 `offline*` + `offline_diff`
//   locks/roles ..... `locks` frame + unicast `error {locked_by}`
//
// Reactive story: the interface itself is framework-light (plain data +
// methods + subscribe). EngineWorld (engineWorld/) bumps `version` and calls
// subscribers imperatively; a `.svelte.ts` wrapper (W1, later) can mirror it
// into runes. FixtureWorld (W1, later) implements the same surface over
// designData/meterSim.

/** Engine param value encoding (mirrors the relay `param` cmd/frame). */
export type ParamValue = number | boolean | string;

/** Honest-UI per-param state (PLAN W2 rule: unknown => dimmed read-only,
 *  unsupported => collapsed via module fallback, never faked). */
export type ParamStatus = 'known' | 'pending' | 'unknown' | 'unsupported';

/** Strip family kinds (engine `topology[].kind` / `scopes` tags). */
export type StripKind = 'in' | 'ret' | 'bus' | 'fxsnd' | 'mtx' | 'main' | 'dca';

/** One indexed caps row (ranges in engineering units, never wire floats). */
export interface CapsRow {
  id: string;
  kind: 'float' | 'bool' | 'enum' | 'text';
  min: number;
  max: number;
  unit: string;
  def: ParamValue;
  label: string;
  /** Strip kinds this id applies to (caps `scopes`; legacy `scope` fallback). */
  scopes: string[];
}

/** One derived bank: a rail button + its strip set (see mapping/banks.ts). */
export interface WorldBank {
  /** Stable key (`ch1`, `mix`, `dca`, …; derived, not engine-sent). */
  key: string;
  /** Rail label (`Ch 1-16`, `Mix`, `Mtx/Main`, …). */
  label: string;
  /** Rail/name-bar colour (CSS hex; design bank palette). */
  colour: string;
  /** Text colour on `colour` (CSS hex). */
  text: string;
  /** Global strip indices in this bank, in rail order. */
  strips: number[];
  /** Engine topology family keys covered (`['ch','ch2']`, …). */
  families: string[];
}

/** One strip header row (mixer strip / channel master strip data). */
export interface WorldStrip {
  /** Global strip index (engine `ch`). */
  ch: number;
  /** Stable string id (`in1`, `mix3`, … derived from topology name_prefix). */
  id: string;
  name: string;
  /** Source patch label (`Lcl 01`, …; `source` param or topology default). */
  source: string;
  /** Name-bar colour (CSS hex). */
  colour: string;
  /** Text colour on `colour` (CSS hex). */
  text: string;
  kind: StripKind;
  /** Wire fader 0..1 (optimistic value while pending). */
  fader: number;
  muted: boolean;
  /** Design pan -1..1 (null when the strip has no pan). */
  pan: number | null;
  /** True while another client holds the strip lock (or self holds it). */
  locked: boolean;
  /** Lock holder client id, or null. */
  lockedBy: string | null;
}

/** One scalar with its honest-UI status. */
export interface FieldView<T> {
  value: T | null;
  status: ParamStatus;
}

/** Design EQ band (LS/PEQ/HS; engine 0..5 types fold onto these, see mapping). */
export interface EqBandView {
  type: 'LS' | 'PEQ' | 'HS';
  /** Centre frequency Hz. */
  f: FieldView<number>;
  /** Gain dB. */
  g: FieldView<number>;
  /** Q. */
  q: FieldView<number>;
  on: FieldView<boolean>;
  /** Raw engine enum 0..5 when known (lossy fold documented in mapping). */
  rawType: number | null;
}

/** Design channel EQ: on + HPF + 4 mapped bands + the engine 5th band. */
export interface EqView {
  on: FieldView<boolean>;
  hpf: {
    on: FieldView<boolean>;
    f: FieldView<number>;
    /** Design slope 12 | 24 dB/oct (engine 0..2 folds onto these). */
    slope: FieldView<12 | 24>;
  };
  /** Design bands 1..4 <-> engine eq1..eq4. */
  bands: [EqBandView, EqBandView, EqBandView, EqBandView];
  /** Engine eq5_* (air band; no design slot — exposed, never faked). */
  band5: EqBandView | null;
}

/** Design gate (5 knobs) — engine carries 3 extra ids (mode/key_src/key_filter). */
export interface GateView {
  on: FieldView<boolean>;
  thr: FieldView<number>;
  range: FieldView<number>;
  att: FieldView<number>;
  hold: FieldView<number>;
  rel: FieldView<number>;
}

/** Design comp (6 knobs) — engine carries 7 extra ids (hold/mode/det/env/mix/auto…). */
export interface CompView {
  on: FieldView<boolean>;
  thr: FieldView<number>;
  ratio: FieldView<number>;
  att: FieldView<number>;
  rel: FieldView<number>;
  knee: FieldView<number>;
  makeup: FieldView<number>;
}

export interface DynView {
  gate: GateView;
  comp: CompView;
}

/** One generic send (design 0..15 <-> engine send1..16_*). */
export interface SendView {
  /** Design 0..1 send value (via the fader law; see mapping). */
  value: FieldView<number>;
  /** Pre (true) / post (false) tap. */
  pre: FieldView<boolean>;
  on: FieldView<boolean>;
}

export interface InputView {
  gain: FieldView<number>;
  trim: FieldView<number>;
  phantom: FieldView<boolean>;
  /** Design `invert` <-> engine `polarity`. */
  invert: FieldView<boolean>;
  delayOn: FieldView<boolean>;
  delayMs: FieldView<number>;
}

/** Meter buffers by global strip slot (shape mirrors meterSim: level/peak
 *  Float32Arrays + clip latch; decoded imperatively, never per-frame
 *  reactive — SPEC 2a). GR in dB reduction >= 0. */
export interface MeterState {
  /** Slots allocated (== stripCount; resized on topology change). */
  size: number;
  /** Current level per slot (linear 0..1; -1 = no data yet). */
  level: Float32Array;
  /** Peak-hold level per slot (linear 0..1). */
  peak: Float32Array;
  /** Clip latch timestamps (ms, performance.now basis). */
  clipUntil: Float64Array;
  /** Gate GR per slot, dB >= 0 (first 16 channels stream; else 0). */
  grGate: Float32Array;
  /** Comp GR per slot, dB >= 0. */
  grComp: Float32Array;
  /** Latest 64-bin spectrum per channel (only streamed channels present). */
  spectrum(ch: number): Float32Array | null;
  /** Negotiated meter taps (`hello {taps:true}`): tap -> per-slot levels. */
  taps: Record<string, Float32Array>;
}

/** Legacy show library (broadcast `shows` frame). */
export interface ShowSceneStrip {
  ch: number;
  name: string;
  fader: number;
  muted: boolean;
}
export interface ShowScene {
  name: string;
  strips: ShowSceneStrip[];
  main_fader: number | null;
}
export interface ShowLib {
  name: string;
  scenes: ShowScene[];
}

/** R2 numbered scene slot. */
export interface SceneEntry {
  number: number;
  name: string;
  current: boolean;
  savedAt: number;
}

/** R2 FX slot state (params normalized 0..1 in the slot type's table order). */
export interface FxSlotView {
  slot: number;
  type: string;
  on: boolean;
  bypass: boolean;
  retLabel: string;
  level: number;
  params: Record<string, number>;
}

/** R2 routing patch row. */
export interface PatchView {
  strip: number;
  group: string;
  index: number;
  label: string;
}

/** R2 device/connection state. */
export interface DeviceView {
  id: string;
  name: string;
  model: string;
  ip: string;
  kind: string;
  state: string;
  fw: string | null;
}
export interface LinkView {
  state: string;
  name: string;
  ip: string;
  fs: number;
  latencyMs: number;
}
export interface InfoView {
  model: string;
  firmware: string;
  sampleRate: number;
  consoleName: string;
  driver: string;
}

/** R2 offline change row. */
export interface OfflineChangeView {
  strip: number;
  id: string;
  from: ParamValue | null;
  to: ParamValue | null;
}

/** Lock/write denial (unicast `error {locked_by}` / `recalled` notice). */
export interface DenyInfo {
  ch: number | null;
  lockedBy: string | null;
  message: string;
}

export type Unsub = () => void;

/**
 * The World: everything the design screens need, backed by the real engine.
 *
 * Conventions: getters return snapshots (arrays/objects are fresh copies);
 * typed-array meter buffers are live and mutated in place (read, don't hold).
 * `set*` methods are optimistic local-first (SPEC 2a): they return false when
 * the write must not be sent (unsupported id / bad kind / locked own strip),
 * else send (leading edge) and mark pending until the engine confirms.
 */
export interface World {
  readonly kind: string;

  // ---- connection ----
  /** Transport-level online flag (socket up). */
  readonly online: boolean;
  /** Active driver id (`sim`, `xair`, `dl32s`, …). */
  readonly driver: string;
  /** Authoritative self id from the unicast `you` frame (null until seen). */
  readonly selfId: string | null;
  /** Open/connect (driver + optional addr); hello negotiation is automatic. */
  connect(driver?: string, addr?: string): void;
  onStatus(cb: (online: boolean) => void): Unsub;
  /** Reactive poke: called after every ingested frame / local edit. */
  subscribe(cb: () => void): Unsub;
  /** Bumped on every change above (Svelte: `$effect` reads it). */
  readonly version: number;

  // ---- topology / banks / strips ----
  /** Total addressable strips (topology end; grows meters/state windows). */
  readonly stripCount: number;
  /** Derived banks in rail order (see mapping/banks.ts). */
  readonly banks: WorldBank[];
  bankFor(ch: number): WorldBank | null;
  strip(ch: number): WorldStrip;
  /** LR main fader wire 0..1 (null when the driver has none / unknown yet). */
  readonly mainFader: number | null;

  // ---- params / caps ----
  readonly capsReady: boolean;
  /** In caps and in scope for the strip (false before caps arrive). */
  supports(id: string, ch?: number): boolean;
  def(id: string): CapsRow | null;
  readonly unsupported: string[];
  /** Optimistic display value (pending wins), else confirmed, else null. */
  param(ch: number, id: string): ParamValue | null;
  status(ch: number, id: string): ParamStatus;
  setParam(ch: number, id: string, v: ParamValue): boolean;
  setFader(ch: number, v01: number): boolean;
  setMute(ch: number, muted: boolean): boolean;
  setMainFader(v01: number): boolean;
  /** Poll-priority hint (debounced, MRU-first; see EngineWorld focus). */
  focus(chs: number[]): void;
  /** Visible strips (bank page + open channel + visible groups). */
  setVisible(chs: number[]): void;
  setOpenChannel(ch: number | null): void;
  /** Expire timed-out optimistic edits (display reverts); returns keys. */
  sweep(nowMs: number): string[];

  // ---- channel detail views (design model over engine ids) ----
  channelEq(ch: number): EqView;
  channelDyn(ch: number): DynView;
  channelSends(ch: number): SendView[];
  channelInput(ch: number): InputView;

  // ---- meters (live typed arrays; never per-frame reactive) ----
  readonly meters: MeterState;

  // ---- fader law (named by caps; default X-Air — see mapping/faderLaw.ts) ----
  readonly lawId: string;
  faderToDb(f: number): number;
  dbToFader(db: number): number;

  // ---- locks / clients ----
  lockedBy(ch: number): string | null;
  lockStrip(ch: number): void;
  unlockStrip(ch: number): void;
  readonly clients: { count: number; writers: string[]; selfId: string | null };
  /** Last lock/write denial (unicast error / recall-over-lock notice). */
  readonly lastDeny: DenyInfo | null;

  // ---- shows / numbered scenes ----
  readonly shows: ShowLib[];
  readonly numberedScenes: SceneEntry[];
  listShows(): void;
  captureScene(show: string, name: string): void;
  recallScene(show: string, name: string, fadeMs?: number): void;
  deleteScene(show: string, name: string): void;
  sceneSave(n: number, name: string): void;
  sceneRecall(n: number, fadeMs?: number): void;
  sceneRename(n: number, name: string): void;
  sceneDelete(n: number): void;
  sceneClear(): void;

  // ---- fx (R2; empty rows / null caps until the engine sends them) ----
  readonly fxSlots: FxSlotView[];
  readonly fxCaps: { slots: number; types: Array<{ id: string; label: string }> } | null;
  fxSetType(slot: number, type: string): void;
  fxParam(slot: number, id: string, v01: number): void;
  fxBypass(slot: number, bypass: boolean): void;
  fxOn(slot: number, on: boolean): void;

  // ---- routing (R2) ----
  readonly patches: PatchView[];
  readonly routingSupported: boolean;
  patchSet(strip: number, group: string, index: number): void;
  patchClear(strip: number): void;

  // ---- devices / link / info / offline (R2) ----
  readonly devices: DeviceView[];
  readonly scannedAt: number;
  scan(): void;
  connectDevice(id: string): void;
  disconnectDesk(): void;
  readonly link: LinkView | null;
  readonly info: InfoView | null;
  readonly offlineDiff: OfflineChangeView[];
  readonly offlineCount: number;
  offline(model: string): void;
  offlinePush(confirm: boolean): void;
  offlineDiscard(): void;

  // ---- debug counters (world-debug screen) ----
  /** performance.now() at connect, or null. */
  readonly connectedAt: number | null;
  /** performance.now() of the first confirmed value, per `ch:id`. */
  firstValueAt(ch: number, id: string): number | null;
  readonly pendingCount: number;
}

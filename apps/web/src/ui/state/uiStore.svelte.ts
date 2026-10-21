import { physicalChannelName } from './channelName.ts';
import { loadOfflineMode, saveOfflineMode } from './live.ts';
import { clearAppStorage } from '../../appStorage.ts';
// D2 State package: Svelte 5 runes store wrapping the pure uiLogic
// transitions over the design's fixture data. Screens read `ui.state`,
// `ui.chans`, `ui.fx` and call `ui.do.*`; binding to the engine happens in
// wave 3 (this store never imports backend code).
//
// Phantom 48V uses the design's 3 s arm timer; tap tempo accumulates tap
// times with the design's 2.5 s window (see tapUpdate).

import {
  applyPatch,
  backLabel,
  backTo,
  cancelPatch,
  clearPatches,
  connectDevice,
  currentChannel,
  expirePhantom,
  flatEq,
  resetEq,
  goBack,
  goConnect,
  goFx,
  goOffline,
  goPatch,
  goRouting,
  goSettings,
  gridCols,
  headerView,
  initialWorld,
  isChannel,
  masterId,
  openDyn,
  openEq,
  openNameEditor,
  closeNameEditor,
  openScenes,
  openPage,
  patchOneToOne,
  patchSrc,
  pickScene,
  recallScene,
  resetEqParam,
  resetFxParam,
  scanDevices,
  selectBusMaster,
  selectChannel,
  setBank,
  setBusPopup,
  setChannelColor,
  setChannelIcon,
  setChannelName,
  setFader,
  setFine,
  setFxParam,
  setFxSel,
  setTheme,
  setFxType,
  setHpfSlope,
  setLocked,
  setMix,
  setMuteEnable,
  setPan,
  setPatchGroup,
  setPatchSel,
  setPeakHold,
  setRouteBank,
  stagePreset,
  stageRoute,
  setSceneConfirm,
  setSelBand,
  setSend,
  setSof,
  setSpecValue,
  setSpp,
  setTab,
  setDynParam,
  resetDyn,
  dynResetDefaults,
  setEqBandType,
  setEqParam,
  stepChannel,
  stripText,
  tabDefs,
  tapFx,
  toggleDelayOn,
  toggleDyn,
  toggleEqBand,
  toggleEqOn,
  toggleFxOn,
  toggleInvert,
  toggleMute,
  toggleSolo,
  togglePhantom,
  toggleSendPre,
  resetPan,
  unpatch,
  unlock,
  withState,
  visibleCount,
  type EqParam,
  type Mode,
  type RouteGroup,
  type Tab,
  type UiState,
  type World,
} from './uiLogic.ts';
import type { ThemePresetId } from '../modules/theme.ts';
import { initialStateFromUrl, parseFixtureUrl, markReady } from './urlBridge.ts';
import { live } from './live.svelte.ts';
import { buildDisplayEq, buildDisplayDyn, buildDisplayInput, displayFields } from '../world/engineWorld/channelViews.ts';
import type { EngineWorld } from '../world/engineWorld/engineWorld.ts';
import {
  bridgeDelayMs,
  bridgeColor,
  bridgeDyn,
  bridgeDynOn,
  bridgeEq,
  bridgeEqOn,
  bridgeEqType,
  bridgeFader,
  bridgeStrip,
  bridgeFxOn,
  bridgeFxParamByIndex,
  bridgeFxType,
  bridgeGainTrim,
  bridgeHpfOn,
  bridgeHpfSlope,
  bridgeInputBool,
  bridgeInsertOn,
  bridgeInsertSel,
  bridgeLrAssign,
  bridgeAutomixWeight,
  bridgeMute,
  bridgeMuteGroupAssign,
  bridgeMuteGroupMute,
  bridgeName,
  bridgePan,
  bridgePatch,
  bridgeSend,
  bridgeSendPre,
  bridgeSendGrpon,
  bridgeSendTap,
  bridgeSolo,
  bridgeSubgroupAssign,
  bridgeSubgroupMute,
  bridgeUnpatch,
  type EqBridgeParam,
} from './engineBridge.ts';
import { fixturePartnerId, livePartnerId } from './stereoPairs.ts';
import { MeterSim, meterIdsFor } from '../fixtures/meterSim.ts';
import { bankByKey, buildChannels, spec, toN, type Channel } from '../fixtures/designData.ts';
import { lawForDriver } from '../world/mapping/faderLaw.ts';
import { fromN } from '../fixtures/designData.ts';
import { dynSpecKey } from './logic/inputDyn.ts';
import { COLOR_HEX, enginePanToDesign, sendDbToDesign01, tapToPre } from '../world/mapping/params.ts';

let world: World = $state(initialWorld());
let taps: number[] = [];
/** Desk-native tap-tempo types (proto-xair tap targets; mirrors FxScreen). */
const XAIR_TAP_TYPES = ['DLY', '3TAP', '4TAP', 'D/RV', 'D/CR', 'D/FL', 'MODD'];
let armTimer: ReturnType<typeof setTimeout> | null = null;

function update(next: World): void {
  if (initialized && !fixtureSession && !enteredSession && !['connect', 'settings'].includes(next.state.mode)) {
    next = withState(next, { mode: 'connect' });
  }
  world = next;
}

function clearArmTimer(): void {
  if (armTimer !== null) {
    clearTimeout(armTimer);
    armTimer = null;
  }
}

/** Rebind meter activity after any data change (cheap; not per frame). */
let meter: MeterSim | null = $state(null);
let frozen = $state(false);

function refreshMeterIds(): void {
  const ids = meterIdsFor(bankItemIds(world.state.bank), world.state.sel, world.state.mix);
  if (!meter || meter.ids.join() !== ids.join()) {
    meter = new MeterSim(ids);
  }
  meter.bind(
    id => world.chans[id]?.act ?? 0,
    id => world.chans[id]?.ph ?? 0,
  );
  if (frozen) meter.freeze();
}

function bankItemIds(key: string): string[] {
  try {
    return bankByKey(key).items.map(i => i.id);
  } catch {
    return [];
  }
}

/** Live engine when a relay link is up with caps (null in fixture mode). */
function liveEngine(): EngineWorld | null {
  const w = live.engine;
  return live.phase === 'online' && w && w.capsReady ? w : null;
}

/**
 * Engine -> fixture live sync (desk-side moves show in the fixture UI).
 *
 * The fixture store stays the display model, but fader/mute are now
 * bidirectional: every engine ingest (write echo, poller diff, or the
 * desk-broadcast fast path) re-runs this sync, copying confirmed engine
 * values into the mapped fixture strips. Bounded optimistic edits own their control during a gesture; release,
 * denial and expiry reveal the latest confirmed desk value. Capability
 * and unknown metadata travels with the actual displayed model.
 * While syncing, `forward` is muted so a desk move is never echoed back
 * to the desk as a new write.
 */
let syncing = false;

/** Engine strip -> fixture id (inverse of `bridgeStrip`; null = no strip).
 * `ret` strips resolve family-aware like `bridgeStrip`: the `aux`-family
 * strip maps to `aux1`, `fxret`-family strips to `fxN`. */
function engineChToFixture(w: EngineWorld, ch: number): string | null {
  const kind = w.kindOf(ch);
  if (!kind) return null;
  if (kind === 'main') return 'mainlr';
  if (kind === 'ret') {
    const auxBank = w.banks.find((b) => b.families.includes('aux'));
    if (auxBank && auxBank.strips.includes(ch)) return 'aux1';
    const fxBank = w.banks.find((b) => b.families.includes('fxret'));
    if (fxBank) {
      const n = fxBank.strips.indexOf(ch) + 1;
      return n >= 1 ? `fx${n}` : null;
    }
    // No `fxret` family on this desk: fall through to the coarse ordinal.
  }
  let n = 1;
  for (let c = 0; c < ch; c++) {
    if (w.kindOf(c) === kind) n++;
  }
  switch (kind) {
    case 'in':
      return `in${n}`;
    case 'bus':
      return `mix${n}`;
    case 'fxsnd':
      return `fxs${n}`;
    case 'dca':
      return `dca${n}`;
    case 'ret':
      return `fx${n}`;
    default:
      return null;
  }
}

/** Live topology fingerprint for the fixture reseed below: driver plus
 * the derived bank shape (EngineWorld never shrinks stripCount, so the
 * bank layout -- not the count -- tells sim-77 apart from xair-36). */
let liveTopoKey: string | null = null;

function liveTopologyKey(w: EngineWorld): string {
  return w.driver + '|' + w.banks.map((b) => b.key + ':' + b.strips.length).join(',');
}

// Display projections contain plain records/arrays and NaN for unknown
// numbers. Compare those directly: stringify allocated two full JSON trees
// per section/strip/ingest and also collapsed NaN and null together.
function sameDisplayValue(a: unknown, b: unknown): boolean {
  if (Object.is(a, b)) return true;
  if (!a || !b || typeof a !== 'object' || typeof b !== 'object') return false;
  if (Array.isArray(a) !== Array.isArray(b)) return false;
  const keys = Object.keys(a);
  if (keys.length !== Object.keys(b).length) return false;
  const left = a as Record<string, unknown>, right = b as Record<string, unknown>;
  return keys.every(key => Object.prototype.hasOwnProperty.call(right, key) && sameDisplayValue(left[key], right[key]));
}

const stripDisplayFields = { fader: 'fader', mute: 'mute', name: 'name', source: 'source', pan: 'pan', color: 'color' };

let projectionFrame: number | null = null;
let projectionWorld: EngineWorld | null = null;

/** Ingest keeps every confirmed/pending parameter in EngineWorld. Project
 * its latest complete state at most once per paint, rather than scanning
 * every strip once for every parameter in a hydration burst. */
function scheduleWorldProjection(w: EngineWorld): void {
  if (typeof window === 'undefined' || typeof window.requestAnimationFrame !== 'function') {
    syncWorldToFixture(w);
    return;
  }
  projectionWorld = w;
  if (projectionFrame !== null) return;
  projectionFrame = window.requestAnimationFrame(() => {
    projectionFrame = null;
    const latest = projectionWorld;
    projectionWorld = null;
    if (latest && latest === live.engine) syncWorldToFixture(latest);
  });
}

function syncWorldToFixture(w: EngineWorld): void {
  if (live.phase !== 'online' || !w.capsReady) return;
  // Same-socket driver switch (sim -> xair): strips the new topology never
  // maps (sim in17..48, and any stale master residue) would otherwise keep
  // the old driver's display values forever -- nothing on the new driver
  // overwrites them (the xair LR scribble is empty, so the falsy-name guard
  // below never heals a stale "CH31" master). Reseed the display channels
  // from the design defaults on a topology change; live values flow back in
  // through the confirmed-only sync below on the same pass.
  const key = liveTopologyKey(w);
  let reseeded = false;
  if (liveTopoKey !== key) {
    liveTopoKey = key;
    const fresh = buildChannels();
    for (const id of Object.keys(world.chans)) {
      if (fresh[id]) world.chans[id] = { ...fresh[id], icon: world.chans[id]?.icon };
    }
    reseeded = true;
  }
  syncing = true;
  try {
    let changed = reseeded;
    for (let ch = 0; ch < w.stripCount; ch++) {
      const id = engineChToFixture(w, ch);
      const slot = id ? world.chans[id] : undefined;
      if (!id || !slot) continue;
      // param() is the shared display truth: bounded local ownership,
      // otherwise the most recent confirmed desk value.
      if (w.supports('fader', ch)) {
        const f = w.param(ch, 'fader');
        if (typeof f === 'number' && typeof slot.fader === 'number' && (!Number.isFinite(slot.fader) || Math.abs(slot.fader - f) > 1e-6)) {
          slot.fader = f;
          changed = true;
        }
      }
      if (w.supports('mute', ch)) {
        const m = w.param(ch, 'mute');
        if (typeof m === 'boolean' && slot.mute !== m) {
          slot.mute = m;
          changed = true;
        }
      }
      // Live solo (monitor bus): sparse engine echoes flow into the
      // fixture solo flag so desk-side solos (and our own echo) show.
      // No pending gate: solo has no optimistic param entry.
      const es = w.soloOf(ch);
      if (slot.solo !== es) {
        slot.solo = es;
        changed = true;
      }
      // Desk name/colour/source/pan ride the same confirmed-only path:
      // pending local edits win, unsupported ids (xair source/link) stay
      // fixture-local because the poller never confirms them.
      if (w.supports('name', ch)) {
        const n = w.param(ch, 'name');
        if (typeof n === 'string' && (slot.rawName !== n || slot.name !== (n.trim() ? n : physicalChannelName(id)))) {
          slot.rawName = n;
          slot.name = n.trim() ? n : physicalChannelName(id);
          changed = true;
        }
      }
      if (w.supports('color', ch)) {
        const c = w.param(ch, 'color');
        if (typeof c === 'number' && Number.isFinite(c)) {
          const hex = COLOR_HEX[Math.min(COLOR_HEX.length - 1, Math.max(0, Math.round(c)))] ?? slot.color;
          if (slot.color !== hex) {
            slot.color = hex;
            slot.text = contrastText(hex);
            changed = true;
          }
        }
      }
      if (w.supports('source', ch)) {
        const s = w.param(ch, 'source');
        if (typeof s === 'string' && slot.src !== s) {
          slot.src = s;
          changed = true;
        }
      }
      const panField = w.supports('balance', ch) ? 'balance' : 'pan';
      if (!w.supports(panField, ch)) {
        if (slot.pan !== null) {
          slot.pan = null;
          changed = true;
        }
      } else if (w.supports(panField, ch)) {
        const p = w.param(ch, panField);
        if (typeof p === 'number' && Number.isFinite(p)) {
          const d = enginePanToDesign(p);
          if (slot.pan === null || Math.abs(slot.pan - d) > 1e-6) {
            slot.pan = d;
            changed = true;
          }
        }
      }
      const eq = buildDisplayEq(w, ch, w.driver === 'sim' || w.link?.state === 'connected');
      if (!sameDisplayValue(slot.eq, eq)) {
        slot.eq = eq;
        changed = true;
      }
      const input = buildDisplayInput(w, ch);
      const dyn = buildDisplayDyn(w, ch);
      const fields = displayFields(w, ch, stripDisplayFields);
      if (!sameDisplayValue(slot.live, fields)) { slot.live = fields; changed = true; }
      if (!sameDisplayValue(slot.input, input)) { slot.input = input; changed = true; }
      if (!sameDisplayValue(slot.dyn, dyn)) { slot.dyn = dyn; changed = true; }
      const slots = Array.from({ length: 16 }, (_, k) => k + 1).filter(n => w.supports(`send${n}_level`, ch));
      const sends = slots.length ? Array.from({ length: Math.max(...slots) }, (_, k) => {
        const v = w.param(ch, `send${k + 1}_level`);
        return w.supports(`send${k + 1}_level`, ch) && typeof v === 'number' ? sendDbToDesign01(v, lawForDriver(w.driver)) : NaN;
      }) : null;
      const pre = sends?.map((_, k) => {
        const v = w.param(ch, `send${k + 1}_tap`);
        return typeof v === 'number' && tapToPre(v, w.driver);
      }) ?? null;
      if (!sameDisplayValue(slot.sends, sends)) { slot.sends = sends; changed = true; }
      if (!sameDisplayValue(slot.sendPre, pre)) { slot.sendPre = pre; changed = true; }
      // XR18's observed return switch is read-only, never an invented source writer.
      const rtn = w.param(ch, 'rtnsw');
      const usb = w.param(ch, 'usbreturn');
      if (!w.supports('source', ch)) {
        const usbLabel = typeof usb === 'number' ? `USB ${usb + 1}` : 'USB return unknown';
        const src = w.kindOf(ch) !== 'in' ? 'Source unavailable' : typeof rtn === 'boolean' ? (rtn ? usbLabel : `Local input · ${usbLabel}`) : `Source unknown · ${usbLabel}`;
        if (slot.src !== src) { slot.src = src; changed = true; }
      }
      // Do not retain previous-epoch/fixture readings while hydration is incomplete.
      for (const f of ['fader'] as const) if (!slot.live.known[f] && Number.isFinite(slot[f])) { slot[f] = NaN; changed = true; }
      if (!slot.live.known.name) {
        const fallback = physicalChannelName(id);
        if (slot.name !== fallback || slot.rawName !== '') { slot.name = fallback; slot.rawName = ''; changed = true; }
      }
    }
    if (changed) update(world);
  } finally {
    syncing = false;
  }
}

// (Re)subscription now fans out through `live.observeWorld` (wired once
// in `initFromUrl`), so no lazy per-world bookkeeping lives here.

// Diagnostics hook (same pattern as live.svelte.ts `__live`): lets
// verification scripts observe fixture fader/mute without touching the UI.
try {
  (
    window as unknown as {
      __fixture?: { fader: (id: string) => unknown; mute: (id: string) => unknown };
    }
  ).__fixture = {
    fader: (id: string) => world.chans[id]?.fader,
    mute: (id: string) => world.chans[id]?.mute,
  };
} catch {
  /* non-browser (node tests) */
}

/** Name-bar ink for a desk colour hex (mirrors EngineWorld.strip). */
function contrastText(hex: string): string {
  const m = /^#([0-9a-f]{6})$/i.exec(hex);
  if (!m) return '#ffffff';
  const n = parseInt(m[1]!, 16);
  const lum = (0.299 * ((n >> 16) & 255) + 0.587 * ((n >> 8) & 255) + 0.114 * (n & 255)) / 255;
  return lum > 0.6 ? '#10240f' : '#ffffff';
}

/**
 * Forward one edit to the desk over the live link. Silent no-op in fixture
 * mode (or when the engine refuses the id); the fixture model stays the
 * source of truth and engine denials surface as world error events.
 * Muted while the engine->fixture sync applies desk moves (no echo-back).
 */
function forward(fn: (w: EngineWorld) => void): void {
  if (syncing) return;
  const w = liveEngine();
  if (!w) return;
  if (w.driver !== 'sim' && w.link?.state === 'disconnected') return;
  try {
    fn(w);
    syncWorldToFixture(w);
  } catch {
    /* ignore: fixture already updated */
  }
}

/**
 * Stereo partner for a paired move (linked-fader pairing). A live world
 * with a per-strip `link` leaf (sim) is authoritative; fixture mode falls
 * back to the fixture src convention (FxRet `FX kL`/`FX kR`). Live but
 * link-unknown (xair: global chlink unsurfaced) resolves null — the link
 * IS the state, never guessed from names.
 */
function stereoPartner(id: string): string | null {
  const w = liveEngine();
  if (w) return livePartnerId(w, world.chans, id);
  return fixturePartnerId(world.chans, id);
}

/** Read back one EQ band value from the fixture model for forwarding. */
function eqBandVal(id: string, band: number, param: EqParam): number | null {
  const eq = world.chans[id]?.eq;
  if (!eq) return null;
  if (band === 0) return param === 'f' ? eq.hpf.f : null;
  const b = eq.bands[band - 1];
  if (!b) return null;
  return b[param];
}

// Deliberate session entry is separate from service readiness and fixture data.
let initialized = false;
let fixtureSession = false;
let enteredSession = false;
let attemptGeneration = 0;
let attemptTimer: ReturnType<typeof setTimeout> | null = null;
let cancelReadyWait: (() => void) | null = null;
let cleanup: Promise<void> | null = null;
let cleanupRequired = false;
type Attempt = {
  generation: number; host: string; label: string; deviceId?: string;
  phase: 'service' | 'connecting' | 'slow' | 'failed'; error: string | null;
  engine: EngineWorld | null; capsBaseline: number; linkBaseline: number; devicesBaseline: number;
  denyBaseline: number;
  manualRows: Set<string>;
};
let attempt: Attempt | null = $state(null);
let flowError: string | null = $state(null);
let cancelling = $state(false);
let addressDraft = $state('');
let scanDenyBaseline = 0;
let scanPending = $state(false);
let scanWorld: EngineWorld | null = null;
let scanDevicesBaseline = 0;

function stopAttemptTimer(): void {
  if (attemptTimer) clearTimeout(attemptTimer);
  attemptTimer = null;
}
function invalidateAttempt(): void {
  ++attemptGeneration;
  stopAttemptTimer();
  cancelReadyWait?.();
  cancelReadyWait = null;
  attempt = null;
  scanPending = false;
  scanWorld = null;
}
function enterSession(): void {
  enteredSession = true;
  update(withState(world, { mode: 'mixer' }));
  refreshMeterIds();
}
function connectionProgress(w: EngineWorld): void {
  if (scanPending && scanWorld === w && (w.devicesVersion > scanDevicesBaseline || w.denyVersion > scanDenyBaseline)) {
    scanPending = false; scanWorld = null;
  }
  if (live.scanning && w.denyVersion > scanDenyBaseline && w.lastDeny)
    live.noteScanFailed(w.lastDeny.message);
  const a = attempt;
  if (!a || a.generation !== attemptGeneration || a.engine !== w || !['connecting', 'slow'].includes(a.phase)) return;
  if (w.denyVersion > a.denyBaseline && w.lastDeny) {
    a.phase = 'failed'; a.error = w.lastDeny.message; stopAttemptTimer(); return;
  }
  const link = w.link;
  const rawHost = a.host.split(':')[0]!.toLowerCase();
  const host = /^[\d.]+$/.test(rawHost) ? rawHost.split('.').map(Number).join('.') : rawHost;
  const row = w.devices.find(d => d.ip === link?.ip);
  // Numeric endpoints match exactly. Hostnames require a newly verified
  // manual row in this attempt; an unrelated changed link is insufficient.
  const matches = /^[\d.]+$/.test(host) ? link?.ip === host :
    w.devicesVersion > a.devicesBaseline && row?.via === 'manual' && !a.manualRows.has(row.id);
  if (!live.deskConnected || !matches || w.linkVersion <= a.linkBaseline || w.capsVersion <= a.capsBaseline) return;
  if (!a.deviceId && (w.devicesVersion <= a.devicesBaseline || !row)) return;
  const confirmedHost = a.host;
  // Consume before navigation/history; recovery or another frame cannot reenter.
  invalidateAttempt();
  cleanupRequired = false;
  live.rememberDesk(confirmedHost, {
    name: link?.name?.trim() || undefined, model: row?.model || undefined,
    resolvedIp: link?.ip || undefined, connectedAt: Date.now(),
  });
  enterSession();
}
async function waitForService(generation: number): Promise<void> {
  await live.connect();
  if (generation !== attemptGeneration) throw new Error('Cancelled');
  if (live.phase === 'online' && live.engine?.capsReady) return;
  await new Promise<void>((resolve, reject) => {
    let off = () => {};
    const finish = (error?: Error) => { clearTimeout(timer); off(); cancelReadyWait = null; error ? reject(error) : resolve(); };
    const timer = setTimeout(() => finish(new Error(live.message || 'Connection service did not become ready.')), 16000);
    cancelReadyWait = () => finish(new Error('Cancelled'));
    off = live.observeWorld(w => {
      if (generation !== attemptGeneration) finish(new Error('Cancelled'));
      else if (live.phase === 'online' && w.capsReady) finish();
    });
  });
}
async function cancelConnection(): Promise<void> {
  if (cleanup) return cleanup;
  const hadDeskOperation = !!attempt?.engine || cleanupRequired || scanPending || live.deskConnected || live.connectionStatus().reconnecting;
  invalidateAttempt();
  enteredSession = false;
  cancelling = true; flowError = null;
  update(withState(world, { mode: 'connect' }));
  cleanup = (async () => {
    if (live.embedded || !hadDeskOperation) {
      // Native close gates writes, drains all pending IPC, then detaches its
      // private engine. Never race a new attach against an in-flight probe.
      await live.disconnect();
    } else {
      // A relay is shared. Keep this socket alive until the ordered disconnect
      // reaches the engine AFTER its probe. Socket close alone is no barrier.
      await live.disconnectDeskAndDrain();
    }
    cleanupRequired = false;
  })().catch(error => {
    cleanupRequired = hadDeskOperation;
    flowError = error instanceof Error ? error.message : String(error);
    throw error;
  }).finally(() => { cancelling = false; cleanup = null; });
  return cleanup;
}
async function startConnection(host: string, deviceId?: string): Promise<void> {
  if (cancelling || scanPending || live.scanning || (attempt && attempt.phase !== 'failed')) return;
  if (attempt || cleanupRequired) {
    try { await cancelConnection(); } catch { return; }
  }
  flowError = null;
  enteredSession = false;
  saveOfflineMode(false);
  update(withState(world, { connected: null, mode: 'connect' }));
  const generation = ++attemptGeneration;
  attempt = { generation, host, label: host, deviceId, phase: 'service', error: null,
    engine: null, capsBaseline: 0, linkBaseline: 0, devicesBaseline: 0, denyBaseline: 0, manualRows: new Set() };
  try {
    await waitForService(generation);
    if (generation !== attemptGeneration || !attempt) return;
    const w = live.engine!;
    const a = attempt;
    a.engine = w; a.capsBaseline = w.capsVersion; a.linkBaseline = w.linkVersion; a.devicesBaseline = w.devicesVersion;
    a.denyBaseline = w.denyVersion;
    a.manualRows = new Set(w.devices.filter(d => d.via === 'manual').map(d => d.id));
    a.phase = 'connecting';
    cleanupRequired = true;
    if (deviceId) {
      if (!w.devices.some(d => d.id === deviceId)) throw new Error('This mixer is no longer in the results. Search again or use its address.');
      w.connectDevice(deviceId);
    } else {
      live.rememberDesk(host);
      w.probeDesk(host);
    }
    attemptTimer = setTimeout(() => {
      if (attempt?.generation === generation && attempt.phase === 'connecting') {
        attempt.phase = 'slow';
        attempt.error = 'No response yet. Retry or cancel.';
      }
    }, 12000);
  } catch (error) {
    if (generation !== attemptGeneration || !attempt) return;
    stopAttemptTimer(); attempt.phase = 'failed';
    attempt.error = error instanceof Error ? error.message : String(error);
  }
}
function manualConnection(raw: string): void {
  const value = raw.trim();
  const [host, port] = value.split(':');
  if (!host || /[\s/]/.test(value) || value.split(':').length > 2 ||
    (port !== undefined && (!/^\d+$/.test(port) || +port < 1 || +port > 65535)) ||
    (/^[\d.]+$/.test(host) && (host.split('.').length !== 4 || host.split('.').some(n => !/^\d{1,3}$/.test(n) || +n > 255))) ||
    !/^[a-z\d](?:[a-z\d.-]*[a-z\d])?$/i.test(host)) {
    flowError = 'Enter a valid IP address or hostname.'; return;
  }
  addressDraft = value;
  void startConnection(value);
}

export const ui = {
  get connectionFlow() { return { attempt, error: flowError, cancelling, cleanupRequired, scanPending }; },
  get connectionAddress() { return addressDraft; },
  set connectionAddress(value: string) { addressDraft = value; },
  get state(): UiState {
    return world.state;
  },
  get chans(): Record<string, Channel> {
    return world.chans;
  },
  get fx() {
    return world.fx;
  },
  get meter(): MeterSim | null {
    return meter;
  },
  get frozen(): boolean {
    return frozen;
  },
  get sel(): Channel {
    const channel = currentChannel(world);
    if (frozen || world.state.connected === 'offline') return channel;
    const w = live.engine;
    const ch = w?.capsReady ? bridgeStrip(w, channel.id) : null;
    if (w && ch !== null && live.phase === 'online' && (w.driver === 'sim' || w.link?.state === 'connected')) return channel;
    if (w && ch !== null) return { ...channel, eq: buildDisplayEq(w, ch, false) };
    // No live topology yet: never show a seeded EQ as a connected desk.
    return { ...channel, eq: channel.eq ? { live: true, on: null, hpf: { on: null, f: NaN, slope: null, supported: false }, bands: [] } : null };
  },
  get masterId(): string {
    return masterId(world);
  },
  get header() {
    // The header dot names the DESK link only (fixture `connected` state):
    // the frontend<->backend link auto-connects at boot and is never shown.
    return headerView(world);
  },
  get backTo(): Mode {
    return backTo(world);
  },
  get backLabel(): string {
    return backLabel(world);
  },
  get tabs() {
    return tabDefs(currentChannel(world));
  },
  get strip() {
    return (id: string) => stripText(world, id);
  },
  get sppCount(): number {
    return visibleCount(world.state.spp);
  },
  get gridCols(): string {
    return gridCols(world.state.spp);
  },

  /** Enter fixture mode from the URL (called once by mount). */
  initFromUrl(search: string): { fixture: boolean; freeze: boolean } {
    if (initialized) return { fixture: fixtureSession, freeze: frozen };
    const r = initialStateFromUrl(search, world.state, world.chans);
    const params = parseFixtureUrl(search);
    fixtureSession = r.fixture;
    initialized = true;
    enteredSession = r.fixture;
    update({ state: r.fixture ? r.state : { ...r.state, mode: params.screen === 'settings' ? 'settings' : 'connect', connected: loadOfflineMode() ? 'offline' : null }, chans: world.chans, fx: world.fx });
    live.observeWorld(w => { connectionProgress(w); scheduleWorldProjection(w); });
    live.initFromUrl(search);
    frozen = r.freeze;
    refreshMeterIds();
    return { fixture: r.fixture, freeze: r.freeze };
  },

  /** Advance meters one tick (or re-freeze in fixture freeze mode). */
  stepMeters(tSec: number, nowMs: number, rand?: () => number): void {
    refreshMeterIds();
    if (!meter) return;
    if (frozen) meter.freeze();
    else meter.step(tSec, nowMs, rand);
  },

  markReady,

  do: {
    openPage: (m: Mode) => {
      // Deliberate navigation uses session truth; a separate pending attempt
      // keeps its freshness checks and ordered cancellation lifecycle.
      if (m === 'mixer' && !cancelling && (live.deskConnected || world.state.connected === 'offline' || fixtureSession)) enterSession();
      else update(openPage(world, m));
    },
    cancelConnection: () => { void cancelConnection().catch(() => undefined); },
    retryConnection: async () => {
      const a = attempt;
      if (!a) return;
      try { await cancelConnection(); } catch { return; }
      await startConnection(a.host, a.deviceId);
    },
    goFx: () => update(goFx(world)),
    goRouting: () => update(goRouting(world)),
    goConnect: () => update(goConnect(world)),
    goSettings: () => update(goSettings(world)),
    goOnline: async () => {
      if (cancelling) return;
      saveOfflineMode(false);
      update(withState(goConnect(world), { connected: null }));
      await live.connect();
    },
    clearAppStorage: async () => {
      clearArmTimer();
      taps = [];
      await cancelConnection();
      await live.disconnect();
      await clearAppStorage();
      liveTopoKey = null;
      update(withState(initialWorld(), { mode: 'connect', connected: null }));
      refreshMeterIds();
      // Reload drops layout/profile/preset/module caches and URL overrides.
      window.location.replace(window.location.pathname + '?screen=connection');
    },
    goOffline: async () => {
      try { await cancelConnection(); } catch {
        // Offline remains a local escape, with an honest remote-cleanup warning.
        await live.disconnect().catch(() => undefined);
      }
      saveOfflineMode(true);
      const demo = buildChannels();
      for (const [id, channel] of Object.entries(world.chans)) {
        if (channel.eq?.live && demo[id]) channel.eq = demo[id].eq;
      }
      liveTopoKey = null;
      update(goOffline(world));
      enterSession();
    },
    goBack: () => update(goBack(world)),
    openScenes: () => update(openScenes(world)),
    goPatch: (id: string) => update(goPatch(world, id)),
    setBank: (b: string) => { update(setBank(world, b)); refreshMeterIds(); },
    openBusPopup: () => update(setBusPopup(world, true)),
    closeBusPopup: () => update(setBusPopup(world, false)),
    selectBusMaster: (id: string) => {
      update(selectBusMaster(world, id));
      refreshMeterIds();
    },
    select: (id: string) => { update(selectChannel(world, id)); refreshMeterIds(); },
    openEq: (id: string) => update(openEq(world, id)),
    openDyn: (id: string) => update(openDyn(world, id)),
    stepChannel: (d: number) => { update(stepChannel(world, d)); refreshMeterIds(); },
    setTab: (t: Tab) => update(setTab(world, t)),
    setSelBand: (b: number) => update(setSelBand(world, b)),
    setFader: (id: string, v: number) => {
      const before = world.chans[id]?.fader;
      update(setFader(world, id, v));
      const nv = world.chans[id]?.fader;
      if (typeof nv !== 'number') return;
      // Linked-fader pairing: the partner takes the same value locally and
      // both writes ride the existing live path (no re-read, no wait).
      let partner: string | null = null;
      if (nv !== before) {
        partner = stereoPartner(id);
        if (partner && world.chans[partner]?.fader !== nv) {
          update(setFader(world, partner, nv));
        }
      }
      forward(w => { bridgeFader(w, id, nv); if (partner) bridgeFader(w, partner, nv); });
    },
    setSend: (id: string, k: number, v: number) => {
      update(setSend(world, id, k, v));
      const nv = world.chans[id]?.sends?.[k];
      if (typeof nv === 'number') forward(w => { bridgeSend(w, id, k, nv); });
    },
    setPan: (id: string, v: number) => {
      update(setPan(world, id, v));
      const nv = world.chans[id]?.pan;
      if (typeof nv === 'number') forward(w => { bridgePan(w, id, nv); });
    },
    resetPan: (id: string) => {
      update(resetPan(world, id));
      forward(w => { bridgePan(w, id, 0); });
    },
    setInputKnob: (id: string, key: 'gain' | 'trim' | 'delay', v: number) => {
      const w = liveEngine();
      if (w) { const ch = bridgeStrip(w, id); if (ch !== null && !world.state.locked) w.setParam(ch, key === 'delay' ? 'delay_ms' : key, v); return; }
      update(setSpecValue(world, id, key, (c: Channel, nv: number) => {
        if (!c.input) return;
        if (key === 'gain') c.input.gain = nv;
        else if (key === 'trim') c.input.trim = nv;
        else c.input.delay = nv;
      }, toN(spec(key), v)));
      const input = world.chans[id]?.input;
      if (!input) return;
      if (key === 'gain') forward(w => { bridgeGainTrim(w, id, 'gain', input.gain); });
      else if (key === 'trim') forward(w => { bridgeGainTrim(w, id, 'trim', input.trim); });
      else forward(w => { bridgeDelayMs(w, id, input.delay); });
    },
    toggleMute: (id: string) => {
      const before = world.chans[id]?.mute;
      update(toggleMute(world, id));
      const nv = world.chans[id]?.mute;
      if (typeof nv !== 'boolean') return;
      // Stereo links gang mute too: the partner ends at the same state
      // locally and both writes ride the existing live path.
      let partner: string | null = null;
      if (nv !== before) {
        partner = stereoPartner(id);
        if (partner && world.chans[partner]?.mute !== nv) {
          update(toggleMute(world, partner));
        }
      }
      forward(w => { bridgeMute(w, id, nv); if (partner) bridgeMute(w, partner, nv); });
    },
    // Desk group membership + masters (desk-only: no fixture counterpart —
    // the channel/mixer scopes read this state straight from the engine, so
    // no fixture update is needed; the command echo re-renders the UI).
    setGroupAssign: (kind: string, id: string, group: number, member: boolean) => {
      if (world.state.locked) return;
      forward(w => {
        if (kind === 'mg') bridgeMuteGroupAssign(w, id, group, member);
        else if (kind === 'sg') bridgeSubgroupAssign(w, id, group, member);
      });
    },
    setGroupMute: (kind: string, group: number, muted: boolean) => {
      if (world.state.locked) return;
      forward(w => {
        if (kind === 'mg') bridgeMuteGroupMute(w, group, muted);
        else if (kind === 'sg') bridgeSubgroupMute(w, group, muted);
      });
    },
    toggleSolo: (id: string) => {
      update(toggleSolo(world, id));
      const nv = world.chans[id]?.solo;
      if (typeof nv === 'boolean') forward(w => { bridgeSolo(w, id, nv); });
    },
    openNameEditor: (id: string) => update(openNameEditor(world, id)),
    closeNameEditor: () => update(closeNameEditor(world)),
    setChannelName: (id: string, name: string) => {
      const clean = name.trim().slice(0, 12);
      // The physical display fallback is never a name command. Forward
      // only the explicit, accepted rename, including when the old scribble is blank.
      if (world.state.locked || !clean || !world.chans[id]) return;
      const before = world.chans[id].rawName ?? world.chans[id].name;
      update(setChannelName(world, id, clean));
      if (world.chans[id].rawName !== undefined) world.chans[id].rawName = clean;
      if (clean !== before) forward(w => { bridgeName(w, id, clean); });
    },
    setChannelColor: (id: string, color: string, text: string) => {
      update(setChannelColor(world, id, color, text));
      const nv = world.chans[id]?.color;
      if (typeof nv === 'string' && nv) forward(w => { bridgeColor(w, id, nv); });
    },
    setChannelIcon: (id: string, icon: string | null) => {
      update(setChannelIcon(world, id, icon));
    },
    setEqParam: (id: string, band: number, param: EqParam, v: number) => {
      update(setEqParam(world, id, band, param, v));
      const nv = eqBandVal(id, band, param);
      if (typeof nv === 'number') forward(w => { bridgeEq(w, id, band, param as EqBridgeParam, nv); });
    },
    resetEqParam: (id: string, band: number, param: EqParam) => {
      update(resetEqParam(world, id, band, param));
      const nv = eqBandVal(id, band, param);
      if (typeof nv === 'number') forward(w => { bridgeEq(w, id, band, param as EqBridgeParam, nv); });
    },
    toggleEqOn: (id: string) => {
      update(toggleEqOn(world, id));
      const nv = world.chans[id]?.eq?.on;
      if (typeof nv === 'boolean') forward(w => { bridgeEqOn(w, id, nv); });
    },
    toggleEqBand: (id: string, band: number) => {
      update(toggleEqBand(world, id, band));
      // Only the HPF has an engine leaf; per-band bypass stays fixture-local.
      if (band === 0) {
        const nv = world.chans[id]?.eq?.hpf.on;
        if (typeof nv === 'boolean') forward(w => { bridgeHpfOn(w, id, nv); });
      }
    },
    flatEq: (id: string) => {
      update(flatEq(world, id));
      const bands = world.chans[id]?.eq?.bands;
      if (bands) forward(w => {
        bands.forEach((b, i) => { bridgeEq(w, id, i + 1, 'g', b.g); });
      });
    },
    resetEq: (id: string) => {
      update(resetEq(world, id));
      const eq = world.chans[id]?.eq;
      if (!eq) return;
      forward(w => {
        bridgeEq(w, id, 0, 'f', eq.hpf.f);
        eq.bands.forEach((b, i) => {
          bridgeEq(w, id, i + 1, 'f', b.f);
          bridgeEq(w, id, i + 1, 'g', b.g);
          bridgeEq(w, id, i + 1, 'q', b.q);
        });
        if (eq.on !== null) bridgeEqOn(w, id, eq.on);
      });
    },
    setHpfSlope: (id: string, s: 12 | 24) => {
      update(setHpfSlope(world, id, s));
      forward(w => { bridgeHpfSlope(w, id, s); });
    },
    setEqBandType: (id: string, band: number, t: import('../fixtures/designData.ts').EqBandType) => {
      update(setEqBandType(world, id, band, t));
      const type = world.chans[id]?.eq?.bands[band - 1]?.type;
      if (type) forward(w => { bridgeEqType(w, id, band, type); });
    },
    toggleDelayOn: (id: string) => {
      const w = liveEngine();
      if (w) { const ch = bridgeStrip(w, id); if (ch !== null && !world.state.locked && typeof w.param(ch, 'delay_on') === 'boolean') w.setParam(ch, 'delay_on', !w.param(ch, 'delay_on')); return; }
      update(toggleDelayOn(world, id));
      const nv = world.chans[id]?.input?.delayOn;
      if (typeof nv === 'boolean') forward(w => { bridgeInputBool(w, id, 'delay_on', nv); });
    },
    toggleInvert: (id: string) => {
      const w = liveEngine();
      if (w) { const ch = bridgeStrip(w, id); if (ch !== null && !world.state.locked && typeof w.param(ch, 'polarity') === 'boolean') w.setParam(ch, 'polarity', !w.param(ch, 'polarity')); return; }
      update(toggleInvert(world, id));
      const nv = world.chans[id]?.input?.invert;
      if (typeof nv === 'boolean') forward(w => { bridgeInputBool(w, id, 'polarity', nv); });
    },
    togglePhantom: (id: string) => {
      clearArmTimer();
      const before = world.state.phantomArm;
      update(togglePhantom(world, id));
      // Forward only the commit tap (arming alone touches nothing).
      if (before === id) {
        const nv = world.chans[id]?.input?.phantom;
        if (typeof nv === 'boolean') forward(w => { bridgeInputBool(w, id, 'phantom', nv); });
      }
      if (before !== id && world.state.phantomArm === id) {
        armTimer = setTimeout(() => {
          update(expirePhantom(world, id));
          armTimer = null;
        }, 3000);
      }
    },
    setDynParam: (id: string, which: 'gate' | 'comp', prop: string, n: number) => {
      const w = liveEngine();
      if (w) { const ch = bridgeStrip(w, id); if (ch !== null && !world.state.locked) w.setParam(ch, `${which}_${prop}`, fromN(spec(dynSpecKey(which, prop)), n)); return; }
      update(setDynParam(world, id, which, prop, n));
      const d = world.chans[id]?.dyn?.[which] as unknown as Record<string, number> | undefined;
      const nv = d?.[prop];
      if (typeof nv === 'number') forward(w => { bridgeDyn(w, id, which, prop, nv); });
    },
    toggleDyn: (id: string, which: 'gate' | 'comp') => {
      update(toggleDyn(world, id, which));
      const nv = world.chans[id]?.dyn?.[which]?.on;
      if (typeof nv === 'boolean') forward(w => { bridgeDynOn(w, id, which, nv); });
    },
    resetDyn: (id: string, which: 'gate' | 'comp') => {
      update(resetDyn(world, id, which));
      const d = world.chans[id]?.dyn?.[which] as unknown as Record<string, unknown> | undefined;
      if (!d) return;
      const defaults = dynResetDefaults(which);
      forward(w => {
        for (const prop of Object.keys(defaults)) {
          const nv = d[prop];
          if (typeof nv === 'number') bridgeDyn(w, id, which, prop, nv);
        }
        if (typeof d['on'] === 'boolean') bridgeDynOn(w, id, which, d['on'] as boolean);
      });
    },
    toggleSendPre: (id: string, k: number) => {
      update(toggleSendPre(world, id, k));
      const nv = world.chans[id]?.sendPre?.[k];
      if (typeof nv === 'boolean') forward(w => { bridgeSendPre(w, id, k, nv); });
    },
    // Desk send group-on (desk-only: no fixture counterpart — the send
    // rows read this state straight from the engine; the echo re-renders).
    // Inverts the current echo-confirmed value (null = unknown -> group on).
    toggleSendGrpon: (id: string, k: number) => {
      if (world.state.locked) return;
      forward(w => {
        const ch = bridgeStrip(w, id);
        if (ch === null) return;
        const cur = w.param(ch, `send${k + 1}_grpon`);
        bridgeSendGrpon(w, id, k, !(cur === true));
      });
    },
    // Desk send tap point (desk-only: no fixture counterpart — the send
    // rows read this state straight from the engine; the echo re-renders).
    setSendTap: (id: string, k: number, value: number) => {
      if (world.state.locked || !Number.isInteger(value)) return;
      forward(w => { bridgeSendTap(w, id, k, value); });
    },
    // Desk mix config (desk-only: LR assign + automix weight ride the
    // engine params; the echo re-renders the card).
    setLrAssign: (id: string, assigned: boolean) => {
      if (world.state.locked || typeof assigned !== 'boolean') return;
      forward(w => { bridgeLrAssign(w, id, assigned); });
    },
    setDcaAssign: (id: string, group: number, member: boolean) => {
      if (world.state.locked || !Number.isInteger(group) || typeof member !== 'boolean') return;
      forward(w => {
        const ch = bridgeStrip(w, id);
        if (ch === null) return;
        w.setParam(ch, `dca${group + 1}_assign`, member);
      });
    },
    setAutomixOn: (id: string, on: boolean) => {
      if (world.state.locked || typeof on !== 'boolean') return;
      forward(w => {
        const ch = bridgeStrip(w, id);
        if (ch === null) return;
        w.setParam(ch, 'automix_on', on);
      });
    },
    setAutomixWeight: (id: string, value: number) => {
      if (world.state.locked || !Number.isFinite(value)) return;
      forward(w => { bridgeAutomixWeight(w, id, Math.min(1, Math.max(0, value))); });
    },
    // Channel insert (desk-only: bypass + raw 0..8 select ride the engine
    // params; the echo re-renders the card).
    setInsertOn: (id: string, on: boolean) => {
      if (world.state.locked || typeof on !== 'boolean') return;
      forward(w => { bridgeInsertOn(w, id, on); });
    },
    setInsertSel: (id: string, value: number) => {
      if (world.state.locked || !Number.isFinite(value)) return;
      forward(w => { bridgeInsertSel(w, id, value); });
    },
    // Desk fire-and-forget clear-solo (monitor bus only).
    clearSolos: () => {
      if (world.state.locked) return;
      forward(w => { w.clearSolo(); });
    },
    setFxSel: (i: number) => update(setFxSel(world, i)),
    setFxType: (i: number, t: string) => {
      update(setFxType(world, i, t));
      forward(w => { bridgeFxType(w, i, t); });
    },
    setFxParam: (i: number, p: number, n: number) => {
      update(setFxParam(world, i, p, n));
      forward(w => { bridgeFxParamByIndex(w, i, p, n); });
    },
    resetFxParam: (i: number, p: number) => {
      update(resetFxParam(world, i, p));
      forward(w => { bridgeFxParamByIndex(w, i, p, 0.5); });
    },
    toggleFxOn: (i: number) => {
      update(toggleFxOn(world, i));
      const nv = world.fx[i]?.on;
      if (typeof nv === 'boolean') forward(w => { bridgeFxOn(w, i, nv); });
    },
    tapFx: (i: number) => {
      const r = tapFx(world, i, taps, Date.now());
      taps = r.taps;
      update(r.world);
      // Write-through: the tapped BPM drives the live Time param (design
      // Delay `time` id; xair DLY `par02`, other tap types `par01` over
      // the assumed-linear 1..3000 ms map). Missing/unknown live slots
      // stay fixture-local.
      if (r.world.fx[i]?.bpm == null) return;
      const bpm = r.world.fx[i]!.bpm!;
      forward(w => {
        const live = w.fxSlots.find(s => s.slot === i + 1);
        if (!live) return;
        const def = w.fxCaps?.types.find(t => t.id === live.type || t.label === live.type);
        if (def?.params.some(p => p.id === 'time')) {
          const p0 = r.world.fx[i]?.p[0];
          if (typeof p0 === 'number') w.fxParam(i + 1, 'time', p0);
          return;
        }
        if (!def || !XAIR_TAP_TYPES.includes(live.type)) return;
        const id = live.type === 'DLY' ? 'par02' : 'par01';
        if (!def.params.some(p => p.id === id)) return;
        const ms = Math.min(3000, Math.max(1, 60000 / bpm));
        w.fxParam(i + 1, id, (ms - 1) / 2999);
      });
    },
    setPatchSel: (id: string) => update(setPatchSel(world, id)),
    setPatchGroup: (g: RouteGroup) => update(setPatchGroup(world, g)),
    setRouteBank: (b: number) => update(setRouteBank(world, b)),
    patchSrc: (label: string) => {
      const target = world.state.patchSel;
      update(patchSrc(world, label));
      forward(w => { bridgePatch(w, target, label); });
    },
    toggleRoute: (id: string, label: string) => {
      update(stageRoute(world, id, label));
    },
    patchOneToOne: (group: RouteGroup) => {
      update(patchOneToOne(world, group));
      forward(w => {
        for (let n = 1; n <= 32; n++) bridgePatch(w, 'in' + n, group + ' ' + String(n).padStart(2, '0'));
      });
    },
    clearPatches: () => {
      update(clearPatches(world));
      forward(w => {
        for (let n = 1; n <= 48; n++) bridgeUnpatch(w, 'in' + n);
      });
    },
    stagePreset: (kind: 'oneToOne' | 'clear', group: RouteGroup) => {
      update(stagePreset(world, kind, group));
    },
    cancelPatch: () => {
      update(cancelPatch(world));
    },
    applyPatch: () => {
      const cells = world.state.pendingPatch?.cells;
      const before = world.chans;
      update(applyPatch(world));
      if (!cells) return;
      forward(w => {
        for (const [id, label] of Object.entries(cells)) {
          if (before[id]?.src === label) continue;
          if (label === '—') bridgeUnpatch(w, id);
          else bridgePatch(w, id, label);
        }
      });
    },
    unpatch: () => {
      const target = world.state.patchSel;
      update(unpatch(world));
      forward(w => { bridgeUnpatch(w, target); });
    },
    scan: (clock: string) => {
      if (attempt || cancelling || cleanupRequired || scanPending || world.state.connected === 'offline') return;
      const w = liveEngine();
      scanDenyBaseline = w?.denyVersion ?? 0;
      scanDevicesBaseline = w?.devicesVersion ?? 0;
      scanWorld = w;
      scanPending = !!w;
      update(scanDevices(world, clock));
      live.noteScanRequested(!!w);
      // Explicit read-only discovery must work even after a desk disconnect.
      // Parameter write guards still go through forward/EngineWorld unchanged.
      w?.scan();
    },
    disconnect: () => { saveOfflineMode(false); update(withState(world, { connected: null })); void cancelConnection().catch(() => undefined); },
    connect: (key: string) => {
      if (fixtureSession) { update(connectDevice(world, key)); return; }
      const desk = live.engine?.devices.find(d => d.id === key);
      if (desk) void startConnection(desk.ip, key);
    },
    /** Real relay link (connection screen relay card; driver only when explicitly chosen). */
    connectRelay: (host?: string, driver?: string) => {
      saveOfflineMode(false);
      if (world.state.connected === 'offline') update(withState(world, { connected: null }));
      return live.connect(host, driver);
    },
    disconnectRelay: () => live.disconnect(),
    setRelayHost: (host: string) => live.setHost(host),
    /** One-tap LAN fix: point the relay at the machine serving this page. */
    usePageHost: () => live.usePageHost(),
    /** Manual add-by-IP: the relay pings the address and connects to the
     * desk when it answers (the verified row appears as `via: manual`;
     * silence stays an honest inline hint — never a blind row). Recent
     * hosts persist. */
    probeDesk: (ip: string) => manualConnection(ip),
    setSpp: (n: number) => update(setSpp(world, n)),
    setFine: (v: boolean) => update(setFine(world, v)),
    setTheme: (t: ThemePresetId) => update(setTheme(world, t)),
    setSof: (v: boolean) => { update(setSof(world, v)); refreshMeterIds(); },
    setMix: (m: number) => { update(setMix(world, m)); refreshMeterIds(); },
    setPeakHold: (v: boolean) => update(setPeakHold(world, v)),
    setMuteEnable: (v: boolean) => update(setMuteEnable(world, v)),
    setLocked: (v: boolean) => update(setLocked(world, v)),
    unlock: () => update(unlock(world)),
    setSceneConfirm: (v: boolean) => update(setSceneConfirm(world, v)),
    pickScene: (i: number) => update(pickScene(world, i)),
    recallScene: () => update(recallScene(world)),
    isChannel: () => isChannel(world.state),
  },
};

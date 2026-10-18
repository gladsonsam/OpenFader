// D2 State package: Svelte 5 runes store wrapping the pure uiLogic
// transitions over the design's fixture data. Screens read `ui.state`,
// `ui.chans`, `ui.fx` and call `ui.do.*`; binding to the engine happens in
// wave 3 (this store never imports backend code).
//
// Phantom 48V uses the design's 3 s arm timer; tap tempo accumulates tap
// times with the design's 2.5 s window (see tapUpdate).

import {
  applyPatch,
  armPatch,
  backLabel,
  backTo,
  cancelPatch,
  clearPatches,
  connectDevice,
  currentChannel,
  disconnect,
  expirePhantom,
  flatEq,
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
  selectChannel,
  setBank,
  setChannelColor,
  setChannelName,
  setFader,
  setFine,
  setFxParam,
  setFxSel,
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
  setSceneConfirm,
  setSelBand,
  setSend,
  setSof,
  setSpecValue,
  setSpp,
  setTab,
  setDynParam,
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
  toggleRoute,
  resetPan,
  unpatch,
  unlock,
  visibleCount,
  type EqParam,
  type Mode,
  type RouteGroup,
  type Tab,
  type UiState,
  type World,
} from './uiLogic.ts';
import { initialStateFromUrl, markReady } from './urlBridge.ts';
import { live } from './live.svelte.ts';
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
  bridgeGainTrim,
  bridgeHpfOn,
  bridgeHpfSlope,
  bridgeInputBool,
  bridgeMute,
  bridgeMuteGroupAssign,
  bridgeMuteGroupMute,
  bridgeName,
  bridgePan,
  bridgePatch,
  bridgeSend,
  bridgeSendPre,
  bridgeSubgroupAssign,
  bridgeSubgroupMute,
  bridgeUnpatch,
  type EqBridgeParam,
} from './engineBridge.ts';
import { MeterSim, meterIdsFor } from '../fixtures/meterSim.ts';
import { bankByKey, spec, toN, type Channel } from '../fixtures/designData.ts';
import { COLOR_HEX, enginePanToDesign } from '../world/mapping/params.ts';

let world: World = $state(initialWorld());
let taps: number[] = [];
let armTimer: ReturnType<typeof setTimeout> | null = null;

function update(next: World): void {
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
 * values into the mapped fixture strips. Strips with an unconfirmed local
 * edit (`pending`) are skipped so a user drag never fights the network;
 * unmapped families (mtx, anything the desk lacks) stay fixture-local.
 * While syncing, `forward` is muted so a desk move is never echoed back
 * to the desk as a new write.
 */
let syncing = false;

/** Engine strip -> fixture id (inverse of `bridgeStrip`; null = no strip). */
function engineChToFixture(w: EngineWorld, ch: number): string | null {
  const kind = w.kindOf(ch);
  if (!kind) return null;
  if (kind === 'main') return 'mainlr';
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

function syncWorldToFixture(w: EngineWorld): void {
  if (live.phase !== 'online' || !w.capsReady) return;
  syncing = true;
  try {
    let changed = false;
    for (let ch = 0; ch < w.stripCount; ch++) {
      const id = engineChToFixture(w, ch);
      const slot = id ? world.chans[id] : undefined;
      if (!id || !slot) continue;
      // param() prefers optimistic values; pending strips are skipped, so
      // this only ever copies confirmed desk truth.
      if (w.status(ch, 'fader') !== 'pending') {
        const f = w.param(ch, 'fader');
        if (typeof f === 'number' && typeof slot.fader === 'number' && Math.abs(slot.fader - f) > 1e-6) {
          slot.fader = f;
          changed = true;
        }
      }
      if (w.status(ch, 'mute') !== 'pending') {
        const m = w.param(ch, 'mute');
        if (typeof m === 'boolean' && slot.mute !== m) {
          slot.mute = m;
          changed = true;
        }
      }
      // Desk name/colour/source/pan ride the same confirmed-only path:
      // pending local edits win, unsupported ids (xair source/link) stay
      // fixture-local because the poller never confirms them.
      if (w.status(ch, 'name') !== 'pending') {
        const n = w.param(ch, 'name');
        if (typeof n === 'string' && n && slot.name !== n) {
          slot.name = n;
          changed = true;
        }
      }
      if (w.status(ch, 'color') !== 'pending') {
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
      if (w.status(ch, 'source') !== 'pending') {
        const s = w.param(ch, 'source');
        if (typeof s === 'string' && s && slot.src !== s) {
          slot.src = s;
          changed = true;
        }
      }
      if (w.status(ch, 'pan') !== 'pending' && slot.pan !== null) {
        const p = w.param(ch, 'pan');
        if (typeof p === 'number' && Number.isFinite(p)) {
          const d = enginePanToDesign(p);
          if (Math.abs(slot.pan - d) > 1e-6) {
            slot.pan = d;
            changed = true;
          }
        }
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
  try {
    fn(w);
  } catch {
    /* ignore: fixture already updated */
  }
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

export const ui = {
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
    return currentChannel(world);
  },
  get masterId(): string {
    return masterId(world);
  },
  get header() {
    const h = headerView(world);
    // The header dot is the one place that must tell the truth about the
    // real socket (fixture `connected` is a console picker until wave 3
    // binds desks to discovery). Live phases override the fixture dot.
    if (live.phase === 'online') {
      return { ...h, connDot: '#5fe060', connAria: `Connection: live via ${live.host}, select device` };
    }
    if (live.phase === 'connecting') {
      return { ...h, connDot: '#f0b44a', connAria: `Connection: connecting to ${live.host}, select device` };
    }
    if (live.phase === 'error') {
      return { ...h, connDot: '#ff4a3d', connAria: `Connection: ${live.message}, select device` };
    }
    return h;
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
    const r = initialStateFromUrl(search, world.state, world.chans);
    update({ state: r.state, chans: world.chans, fx: world.fx });
    live.initFromUrl(search);
    // Engine -> fixture live sync: desk-side fader/mute moves flow into the
    // fixture display model (no-op offline; pending local edits win).
    live.observeWorld((w) => syncWorldToFixture(w));
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
    openPage: (m: Mode) => update(openPage(world, m)),
    goFx: () => update(goFx(world)),
    goRouting: () => update(goRouting(world)),
    goConnect: () => update(goConnect(world)),
    goSettings: () => update(goSettings(world)),
    goOffline: () => update(goOffline(world)),
    goBack: () => update(goBack(world)),
    openScenes: () => update(openScenes(world)),
    goPatch: (id: string) => update(goPatch(world, id)),
    setBank: (b: string) => { update(setBank(world, b)); refreshMeterIds(); },
    select: (id: string) => { update(selectChannel(world, id)); refreshMeterIds(); },
    openEq: (id: string) => update(openEq(world, id)),
    openDyn: (id: string) => update(openDyn(world, id)),
    stepChannel: (d: number) => { update(stepChannel(world, d)); refreshMeterIds(); },
    setTab: (t: Tab) => update(setTab(world, t)),
    setSelBand: (b: number) => update(setSelBand(world, b)),
    setFader: (id: string, v: number) => {
      update(setFader(world, id, v));
      const nv = world.chans[id]?.fader;
      if (typeof nv === 'number') forward(w => { bridgeFader(w, id, nv); });
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
      update(toggleMute(world, id));
      const nv = world.chans[id]?.mute;
      if (typeof nv === 'boolean') forward(w => { bridgeMute(w, id, nv); });
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
    toggleSolo: (id: string) => update(toggleSolo(world, id)),
    openNameEditor: (id: string) => update(openNameEditor(world, id)),
    closeNameEditor: () => update(closeNameEditor(world)),
    setChannelName: (id: string, name: string) => {
      update(setChannelName(world, id, name));
      const nv = world.chans[id]?.name;
      if (typeof nv === 'string' && nv) forward(w => { bridgeName(w, id, nv); });
    },
    setChannelColor: (id: string, color: string, text: string) => {
      update(setChannelColor(world, id, color, text));
      const nv = world.chans[id]?.color;
      if (typeof nv === 'string' && nv) forward(w => { bridgeColor(w, id, nv); });
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
    setHpfSlope: (id: string, s: 12 | 24) => {
      update(setHpfSlope(world, id, s));
      forward(w => { bridgeHpfSlope(w, id, s); });
    },
    setEqBandType: (id: string, band: number, t: 'LS' | 'PEQ' | 'HS') => {
      update(setEqBandType(world, id, band, t));
      forward(w => { bridgeEqType(w, id, band, t); });
    },
    toggleDelayOn: (id: string) => {
      update(toggleDelayOn(world, id));
      const nv = world.chans[id]?.input?.delayOn;
      if (typeof nv === 'boolean') forward(w => { bridgeInputBool(w, id, 'delay_on', nv); });
    },
    toggleInvert: (id: string) => {
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
    toggleSendPre: (id: string, k: number) => {
      update(toggleSendPre(world, id, k));
      const nv = world.chans[id]?.sendPre?.[k];
      if (typeof nv === 'boolean') forward(w => { bridgeSendPre(w, id, k, nv); });
    },
    setFxSel: (i: number) => update(setFxSel(world, i)),
    setFxType: (i: number, t: string) => update(setFxType(world, i, t)),
    setFxParam: (i: number, p: number, n: number) => update(setFxParam(world, i, p, n)),
    resetFxParam: (i: number, p: number) => update(resetFxParam(world, i, p)),
    toggleFxOn: (i: number) => update(toggleFxOn(world, i)),
    tapFx: (i: number) => {
      const r = tapFx(world, i, taps, Date.now());
      taps = r.taps;
      update(r.world);
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
      update(toggleRoute(world, id, label));
      const cur = world.chans[id]?.src;
      forward(w => {
        if (cur === label) bridgePatch(w, id, label);
        else bridgeUnpatch(w, id);
      });
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
    armPatch: (kind: 'oneToOne' | 'clear', group: RouteGroup) => {
      update(armPatch(world, kind, group));
    },
    cancelPatch: () => {
      update(cancelPatch(world));
    },
    applyPatch: () => {
      const p = world.state.pendingPatch;
      update(applyPatch(world));
      if (!p) return;
      forward(w => {
        if (p.kind === 'oneToOne') {
          for (let n = 1; n <= 32; n++) bridgePatch(w, 'in' + n, p.group + ' ' + String(n).padStart(2, '0'));
        } else {
          for (let n = 1; n <= 48; n++) bridgeUnpatch(w, 'in' + n);
        }
      });
    },
    unpatch: () => {
      const target = world.state.patchSel;
      update(unpatch(world));
      forward(w => { bridgeUnpatch(w, target); });
    },
    scan: (clock: string) => {
      update(scanDevices(world, clock));
      forward(w => { w.scan(); });
    },
    disconnect: () => {
      const was = world.state.connected;
      update(disconnect(world));
      if (was) forward(w => { w.disconnectDesk(); });
    },
    connect: (key: string) => {
      const before = world.state.connected;
      update(connectDevice(world, key));
      if (world.state.connected !== before && world.state.connected) {
        forward(w => { w.connectDevice(world.state.connected as string); });
      }
    },
    /** Real relay link (connection screen relay card; driver only when explicitly chosen). */
    connectRelay: (host?: string, driver?: string) => live.connect(host, driver),
    disconnectRelay: () => live.disconnect(),
    setRelayHost: (host: string) => live.setHost(host),
    setSpp: (n: number) => update(setSpp(world, n)),
    setFine: (v: boolean) => update(setFine(world, v)),
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

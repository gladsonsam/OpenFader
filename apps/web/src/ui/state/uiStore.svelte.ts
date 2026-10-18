// D2 State package: Svelte 5 runes store wrapping the pure uiLogic
// transitions over the design's fixture data. Screens read `ui.state`,
// `ui.chans`, `ui.fx` and call `ui.do.*`; binding to the engine happens in
// wave 3 (this store never imports backend code).
//
// Phantom 48V uses the design's 3 s arm timer; tap tempo accumulates tap
// times with the design's 2.5 s window (see tapUpdate).

import {
  backLabel,
  backTo,
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
  openScenes,
  openPage,
  patchSrc,
  pickScene,
  recallScene,
  resetEqParam,
  resetFxParam,
  scanDevices,
  selectChannel,
  setBank,
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
  togglePhantom,
  toggleSendPre,
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
import { MeterSim, meterIdsFor } from '../fixtures/meterSim.ts';
import { bankByKey, spec, toN, type Channel } from '../fixtures/designData.ts';

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
    const r = initialStateFromUrl(search, world.state, world.chans);
    update({ state: r.state, chans: world.chans, fx: world.fx });
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
    setFader: (id: string, v: number) => update(setFader(world, id, v)),
    setSend: (id: string, k: number, v: number) => update(setSend(world, id, k, v)),
    setPan: (id: string, v: number) => update(setPan(world, id, v)),
    resetPan: (id: string) => update(resetPan(world, id)),
    setInputKnob: (id: string, key: 'gain' | 'trim' | 'delay', v: number) =>
      update(setSpecValue(world, id, key, (c: Channel, nv: number) => {
        if (!c.input) return;
        if (key === 'gain') c.input.gain = nv;
        else if (key === 'trim') c.input.trim = nv;
        else c.input.delay = nv;
      }, toN(spec(key), v))),
    toggleMute: (id: string) => update(toggleMute(world, id)),
    setEqParam: (id: string, band: number, param: EqParam, v: number) => update(setEqParam(world, id, band, param, v)),
    resetEqParam: (id: string, band: number, param: EqParam) => update(resetEqParam(world, id, band, param)),
    toggleEqOn: (id: string) => update(toggleEqOn(world, id)),
    toggleEqBand: (id: string, band: number) => update(toggleEqBand(world, id, band)),
    flatEq: (id: string) => update(flatEq(world, id)),
    setHpfSlope: (id: string, s: 12 | 24) => update(setHpfSlope(world, id, s)),
    setEqBandType: (id: string, band: number, t: 'LS' | 'PEQ' | 'HS') => update(setEqBandType(world, id, band, t)),
    toggleDelayOn: (id: string) => update(toggleDelayOn(world, id)),
    toggleInvert: (id: string) => update(toggleInvert(world, id)),
    togglePhantom: (id: string) => {
      clearArmTimer();
      const before = world.state.phantomArm;
      update(togglePhantom(world, id));
      if (before !== id && world.state.phantomArm === id) {
        armTimer = setTimeout(() => {
          update(expirePhantom(world, id));
          armTimer = null;
        }, 3000);
      }
    },
    setDynParam: (id: string, which: 'gate' | 'comp', prop: string, n: number) =>
      update(setDynParam(world, id, which, prop, n)),
    toggleDyn: (id: string, which: 'gate' | 'comp') => update(toggleDyn(world, id, which)),
    toggleSendPre: (id: string, k: number) => update(toggleSendPre(world, id, k)),
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
    patchSrc: (label: string) => update(patchSrc(world, label)),
    unpatch: () => update(unpatch(world)),
    scan: (clock: string) => update(scanDevices(world, clock)),
    disconnect: () => update(disconnect(world)),
    connect: (key: string) => update(connectDevice(world, key)),
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

// D2 State package: pure (rune-free, DOM-free) port of the design's state
// machine. Mirrors the state fields and every action in
// docs/design/src/design-logic.js.txt (openPage/goBack/goFx/goRouting/
// goConnect/goSettings/goPatch/goOffline, lock handling, scene confirm flow,
// phantom arm, SOF, fine, mix target, routing patch, scan...), plus derived
// selectors screens read (visible strips, current channel, back label, page
// titles, header state).
//
// Lock semantics mirror the design exactly: move()/mute/patch/dyn-toggle and
// the input toggles check `locked`; navigation, EQ edits, FX edits, scene
// picks and settings never do.

import {
  bankByKey,
  bankDefs,
  buildChannels,
  buildFx,
  clamp,
  compSummary,
  delayDistanceText,
  devices,
  eqBandTexts,
  faderAria,
  fmtDb,
  fmtF,
  fmtG,
  fmtMs,
  fromN,
  fxKnobTexts,
  fxRetLabel,
  fxStateLabel,
  gateSummary,
  delayKnobTexts,
  inputKnobTexts,
  hpfTexts,
  panAria,
  panText,
  scenes,
  sendAria,
  spec,
  tapUpdate,
  toN,
  type Channel,
  type FxSlot,
} from '../fixtures/designData.ts';

export type Mode = 'mixer' | 'channel' | 'fx' | 'routing' | 'connect' | 'settings' | 'scenes';
export type Tab = 'input' | 'eq' | 'gate' | 'comp' | 'sends';
export type RouteGroup = 'Lcl' | 'Net' | 'USB';

export const PAGE_MODES: Mode[] = ['fx', 'routing', 'connect', 'settings', 'scenes'];

export const PAGE_TITLES: Record<string, string> = {
  scenes: 'Scenes',
  fx: 'Effects',
  routing: 'Routing',
  connect: 'Connection',
  settings: 'Settings',
};

export interface UiState {
  mode: Mode;
  prevMode: Mode;
  tab: Tab;
  bank: string;
  sel: string;
  selBand: number;
  fine: boolean;
  muteEnable: boolean;
  sof: boolean;
  mix: number;
  scene: number;
  scenePv: number;
  /** Strips-per-screen override (null = prop default, 12). */
  spp: number | null;
  peakHold: boolean;
  locked: boolean;
  sceneConfirm: boolean;
  phantomArm: string | null;
  fxSel: number;
  patchSel: string;
  patchGroup: RouteGroup;
  routeBank: number;
  /** Connected device key, 'offline', or null. */
  connected: string | null;
  lastScan: string;
}

export interface World {
  state: UiState;
  chans: Record<string, Channel>;
  fx: FxSlot[];
}

export function initialUiState(): UiState {
  return {
    mode: 'mixer', prevMode: 'mixer', tab: 'eq', bank: 'ch1', sel: 'in1', selBand: 1,
    fine: false, muteEnable: true, sof: false, mix: 0, scene: 4, scenePv: 4,
    spp: null, peakHold: true, locked: false, sceneConfirm: true, phantomArm: null,
    fxSel: 0, patchSel: 'in1', patchGroup: 'Lcl', routeBank: 0,
    connected: 'foh', lastScan: 'Scanned at start-up',
  };
}

export function initialWorld(): World {
  return { state: initialUiState(), chans: buildChannels(), fx: buildFx() };
}

// ---------- immutable update helpers ----------

function withState(w: World, patch: Partial<UiState>): World {
  return { state: { ...w.state, ...patch }, chans: w.chans, fx: w.fx };
}

function withChannel(w: World, id: string, fn: (c: Channel) => void): World {
  const prev = w.chans[id];
  if (!prev) return w;
  const next: Channel = JSON.parse(JSON.stringify(prev));
  fn(next);
  return { state: w.state, chans: { ...w.chans, [id]: next }, fx: w.fx };
}

function withFx(w: World, i: number, fn: (s: FxSlot) => void): World {
  if (i < 0 || i >= w.fx.length) return w;
  const fx = w.fx.map((x, k) => (k === i ? JSON.parse(JSON.stringify(x)) : x));
  fn(fx[i]);
  return { state: w.state, chans: w.chans, fx };
}

// ---------- navigation (port of openPage + header actions) ----------

export function openPage(w: World, mode: Mode, extra?: Partial<UiState>): World {
  const prev = PAGE_MODES.includes(w.state.mode) ? w.state.prevMode : w.state.mode;
  return withState(w, { mode, prevMode: prev, ...(extra ?? {}) });
}

export function goFx(w: World): World { return openPage(w, 'fx'); }
export function goRouting(w: World): World { return openPage(w, 'routing'); }
export function goConnect(w: World): World { return openPage(w, 'connect'); }
export function goSettings(w: World): World { return openPage(w, 'settings'); }
export function goOffline(w: World): World { return withState(w, { connected: 'offline' }); }

/** Port of goBack: back target depends on prevMode. */
export function backTo(w: World): Mode {
  return isChannel(w.state) ? 'mixer' : (w.state.prevMode === 'channel' ? 'channel' : 'mixer');
}

export function goBack(w: World): World {
  return withState(w, { mode: backTo(w) });
}

export function openScenes(w: World): World {
  if (w.state.mode === 'scenes') return w;
  return openPage(w, 'scenes', { scenePv: w.state.scene });
}

/** Input tab "Change patch in Routing" (port of goPatch closure). */
export function goPatch(w: World, id: string): World {
  const n = parseInt(id.replace(/^\D+/, ''), 10);
  const bank = Number.isFinite(n) ? Math.floor((n - 1) / 16) : 0;
  return openPage(w, 'routing', { patchSel: id, routeBank: clamp(bank, 0, 2) });
}

export function setBank(w: World, bank: string): World {
  bankByKey(bank); // throws on unknown keys, like the design would break
  return withState(w, { bank });
}

export function selectChannel(w: World, id: string): World {
  if (!w.chans[id]) return w;
  return withState(w, { sel: id });
}

/** Channel prev/next arrows cycle over every EQ-capable channel id. */
export function channelOrder(): string[] {
  const order: string[] = [];
  bankDefs().forEach(b => b.items.forEach(it => { if (it.eq) order.push(it.id); }));
  return order;
}

export function stepChannel(w: World, delta: number): World {
  const order = channelOrder();
  const idx = Math.max(0, order.indexOf(w.state.sel));
  const nid = order[(idx + delta + order.length) % order.length];
  return withState(w, { sel: nid });
}

export function openEq(w: World, id: string): World {
  if (!w.chans[id]?.eq) return w;
  return withState(w, { sel: id, mode: 'channel', tab: 'eq' });
}

export function openDyn(w: World, id: string): World {
  if (!w.chans[id]?.eq) return w;
  return withState(w, { sel: id, mode: 'channel', tab: 'comp' });
}

export function setTab(w: World, tab: Tab): World {
  return withState(w, { tab });
}

export function setSelBand(w: World, band: number): World {
  return withState(w, { selBand: band });
}

// ---------- fader / pan / mute (lock-checked, ports of move/onMute) ----------

export function dragFine(w: World): number {
  return w.state.fine ? 0.25 : 1;
}

/** Port of the fader branch of move(): drag pixels -> new 0..1 value. */
export function faderFromDrag(start: number, dyPx: number, trackH: number, fine: boolean): number {
  return clamp(start + (dyPx / trackH) * (fine ? 0.25 : 1), 0, 1);
}

export function setFader(w: World, id: string, v: number): World {
  if (w.state.locked) return w;
  return withChannel(w, id, c => { c.fader = clamp(v, 0, 1); });
}

export function setSend(w: World, id: string, k: number, v: number): World {
  if (w.state.locked) return w;
  return withChannel(w, id, c => {
    if (c.sends) c.sends[k] = Math.round(clamp(v, 0, 1) * 1000) / 1000;
  });
}

/** Port of the pan branch of move(). */
export function panFromDrag(start: number, dxPx: number, trackW: number, fine: boolean): number {
  let v = clamp(start + (dxPx / trackW) * 2 * (fine ? 0.25 : 1), -1, 1);
  if (Math.abs(v) < 0.03) v = 0;
  return Math.round(v * 100) / 100;
}

export function setPan(w: World, id: string, v: number): World {
  if (w.state.locked) return w;
  return withChannel(w, id, c => { if (c.pan != null) c.pan = Math.round(clamp(v, -1, 1) * 100) / 100; });
}

export function resetPan(w: World, id: string): World {
  if (w.state.locked) return w;
  return withChannel(w, id, c => { if (c.pan != null) c.pan = 0; });
}

export function toggleMute(w: World, id: string): World {
  if (!w.state.muteEnable || w.state.locked) return w;
  return withChannel(w, id, c => { c.mute = !c.mute; });
}

// ---------- EQ edits (not lock-checked in the design) ----------

export type EqParam = 'f' | 'g' | 'q';

/** Port of the knob branch of move() for EQ bands (band 0 = HPF). */
export function eqParamFromDrag(param: EqParam, band: number, start: number, dyPx: number, fine: boolean): number {
  const dy = dyPx * (fine ? 0.25 : 1);
  if (param === 'f') return clamp(start * Math.pow(2, dy / 30), 20, band === 0 ? 1000 : 20000);
  if (param === 'g') return Math.round(clamp(start + dy * 0.1, -15, 15) * 10) / 10;
  return Math.round(clamp(start * Math.pow(2, dy / 60), 0.3, 16) * 100) / 100;
}

/** Port of the node branch of move() (graph drag). */
export function eqNodeFromPos(_band: number, x: number, y: number): { f: number; g: number } {
  const cx = clamp(x, 0, 1), cy = clamp(y, 0, 1);
  return {
    f: 20 * Math.pow(1000, cx),
    g: Math.round((15 - 30 * cy) * 10) / 10,
  };
}

export function setEqParam(w: World, id: string, band: number, param: EqParam, v: number): World {
  return withChannel(w, id, c => {
    if (!c.eq) return;
    if (band === 0) c.eq.hpf.f = v;
    else {
      const b = c.eq.bands[band - 1];
      if (b) b[param] = v;
    }
  });
}

const RESET_F = [80, 100, 400, 2500, 8000];
const RESET_Q = [0.7, 0.7, 1.4, 1.4, 0.7];

/** Port of resetParam (double-tap reset). */
export function resetEqParam(w: World, id: string, band: number, param: EqParam): World {
  return withChannel(w, id, c => {
    if (!c.eq) return;
    if (band === 0) { c.eq.hpf.f = 80; return; }
    const b = c.eq.bands[band - 1];
    if (!b) return;
    if (param === 'f') b.f = RESET_F[band];
    else if (param === 'g') b.g = 0;
    else b.q = RESET_Q[band];
  });
}

export function toggleEqOn(w: World, id: string): World {
  return withChannel(w, id, c => { if (c.eq) c.eq.on = !c.eq.on; });
}

export function toggleEqBand(w: World, id: string, bandIndex: number): World {
  return withChannel(w, id, c => {
    if (!c.eq) return;
    if (bandIndex === 0) c.eq.hpf.on = !c.eq.hpf.on;
    else {
      const b = c.eq.bands[bandIndex - 1];
      if (b) b.on = !b.on;
    }
  });
}

export function flatEq(w: World, id: string): World {
  return withChannel(w, id, c => { c.eq?.bands.forEach(b => { b.g = 0; }); });
}

export function setHpfSlope(w: World, id: string, slope: 12 | 24): World {
  return withChannel(w, id, c => { if (c.eq) c.eq.hpf.slope = slope; });
}

export function setEqBandType(w: World, id: string, bandIndex: number, type: 'LS' | 'PEQ' | 'HS'): World {
  return withChannel(w, id, c => {
    const b = c.eq?.bands[bandIndex];
    if (b) b.type = type;
  });
}

// ---------- input tab (lock-checked toggles; spec drag math) ----------

/** Port of the 'g' branch of move(): normalised spec drag -> value. */
export function specFromDrag(specKey: string, startN: number, dyPx: number, fine: boolean): number {
  const s = spec(specKey);
  return fromN(s, clamp(startN + dyPx / 180 * (fine ? 0.25 : 1), 0, 1));
}

export function setSpecValue(w: World, id: string, specKey: string, apply: (c: Channel, v: number) => void, n: number): World {
  void specKey;
  return withChannel(w, id, c => { apply(c, fromN(spec(specKey), clamp(n, 0, 1))); });
}

export function toggleDelayOn(w: World, id: string): World {
  if (w.state.locked) return w;
  return withChannel(w, id, c => { if (c.input) c.input.delayOn = !c.input.delayOn; });
}

export function toggleInvert(w: World, id: string): World {
  if (w.state.locked) return w;
  return withChannel(w, id, c => { if (c.input) c.input.invert = !c.input.invert; });
}

/**
 * Two-tap 48V arm flow (port of togglePhantom without the timer): first tap
 * arms, second tap commits. Expiry is via expirePhantom, driven by the store's
 * 3 s timer exactly like the design's armTimer.
 */
export function togglePhantom(w: World, id: string): World {
  if (w.state.locked) return w;
  if (w.state.phantomArm === id) {
    const armed = withChannel(w, id, c => { if (c.input) c.input.phantom = !c.input.phantom; });
    return withState(armed, { phantomArm: null });
  }
  return withState(w, { phantomArm: id });
}

export function expirePhantom(w: World, id: string): World {
  if (w.state.phantomArm !== id) return w;
  return withState(w, { phantomArm: null });
}

export function phantomLabel(w: World, ch: Channel): string {
  const armed = w.state.phantomArm === ch.id;
  const on = ch.input?.phantom ?? false;
  if (armed) return on ? 'Tap again: 48V off' : 'Tap again: 48V on';
  return on ? '48V On' : '48V Off';
}

// ---------- dynamics (toggle lock-checked; values via spec math) ----------

export function setDynParam(
  w: World, id: string, which: 'gate' | 'comp', prop: string, n: number,
): World {
  const key = dynSpecKey(which, prop);
  return withChannel(w, id, c => {
    const d = c.dyn?.[which] as unknown as Record<string, number> | undefined;
    if (d) d[prop] = fromN(spec(key), clamp(n, 0, 1));
  });
}

export function dynSpecKey(which: 'gate' | 'comp', prop: string): string {
  const table: Record<string, string> = {
    'gate.thr': 'gthr', 'gate.range': 'range', 'gate.att': 'gatt', 'gate.hold': 'hold', 'gate.rel': 'grel',
    'comp.thr': 'cthr', 'comp.ratio': 'ratio', 'comp.knee': 'knee', 'comp.att': 'catt', 'comp.rel': 'crel', 'comp.makeup': 'makeup',
  };
  const key = table[which + '.' + prop];
  if (!key) throw new Error('unknown dyn param: ' + which + '.' + prop);
  return key;
}

export function toggleDyn(w: World, id: string, which: 'gate' | 'comp'): World {
  if (w.state.locked) return w;
  return withChannel(w, id, c => {
    const d = c.dyn?.[which];
    if (d) d.on = !d.on;
  });
}

// ---------- sends ----------

export function toggleSendPre(w: World, id: string, k: number): World {
  if (w.state.locked) return w;
  return withChannel(w, id, c => { if (c.sendPre) c.sendPre[k] = !c.sendPre[k]; });
}

/** Port of the 'h' branch of move(): horizontal send drag -> 0..1. */
export function sendFromDrag(start: number, dxPx: number, trackW: number, fine: boolean): number {
  return clamp(start + (dxPx / trackW) * (fine ? 0.25 : 1), 0, 1);
}

// ---------- FX ----------

export function setFxSel(w: World, i: number): World {
  if (i < 0 || i >= w.fx.length) return w;
  return withState(w, { fxSel: i });
}

export function setFxType(w: World, i: number, type: string): World {
  return withFx(w, i, s => { s.type = type; });
}

export function setFxParam(w: World, i: number, param: number, n: number): World {
  return withFx(w, i, s => {
    s.p[param] = Math.round(clamp(n, 0, 1) * 1000) / 1000;
    if (param === 0) s.bpm = null;
  });
}

export function toggleFxOn(w: World, i: number): World {
  return withFx(w, i, s => { s.on = !s.on; });
}

export function resetFxParam(w: World, i: number, param: number): World {
  return withFx(w, i, s => { s.p[param] = 0.5; });
}

/** Tap tempo with injected tap list + clock (store owns the list). */
export function tapFx(w: World, i: number, prevTaps: number[], now: number): { world: World; taps: number[] } {
  const { taps, bpm, p0 } = tapUpdate(prevTaps, now);
  if (bpm == null || p0 == null) return { world: w, taps };
  const world = withFx(w, i, s => { s.bpm = bpm; s.p[0] = p0; });
  return { world, taps };
}

// ---------- routing ----------

export function setPatchSel(w: World, id: string): World {
  return withState(w, { patchSel: id });
}

export function setPatchGroup(w: World, g: RouteGroup): World {
  return withState(w, { patchGroup: g });
}

export function setRouteBank(w: World, b: number): World {
  return withState(w, { routeBank: clamp(Math.round(b), 0, 2) });
}

/** Channel ids shown in the routing tile grid for a route bank. */
export function routeBankIds(routeBank: number): string[] {
  return Array.from({ length: 16 }, (_, k) => 'in' + (routeBank * 16 + k + 1));
}

/** Source tile labels for a patch group (32 sockets, `Lcl 01` style). */
export function srcTileLabels(patchGroup: RouteGroup): string[] {
  return Array.from({ length: 32 }, (_, k) => patchGroup + ' ' + String(k + 1).padStart(2, '0'));
}

/** Port of routing next(): patching advances to the next channel, wrapping. */
export function nextPatchSel(patchSel: string): { patchSel: string; routeBank: number } {
  const n = parseInt(patchSel.replace(/^\D+/, ''), 10);
  const nn = Number.isFinite(n) ? (n % 48) + 1 : 1;
  return { patchSel: 'in' + nn, routeBank: Math.floor((nn - 1) / 16) };
}

export function patchSrc(w: World, label: string): World {
  if (w.state.locked) return w;
  const id = w.state.patchSel;
  const patched = withChannel(w, id, c => { c.src = label; });
  return withState(patched, nextPatchSel(id));
}

export function unpatch(w: World): World {
  if (w.state.locked) return w;
  return withChannel(w, w.state.patchSel, c => { c.src = '—'; });
}

// ---------- connection ----------

export function scanDevices(w: World, clock: string): World {
  return withState(w, { lastScan: 'Scanned at ' + clock });
}

export function disconnect(w: World): World {
  return withState(w, { connected: null });
}

export function connectDevice(w: World, key: string): World {
  if (!devices().some(d => d.key === key)) return w;
  if (w.state.connected === key) return w;
  return withState(w, { connected: key });
}

// ---------- settings ----------

export function setSpp(w: World, spp: number): World {
  return withState(w, { spp });
}

export function setFine(w: World, v: boolean): World { return withState(w, { fine: v }); }
export function setSof(w: World, v: boolean): World { return withState(w, { sof: v }); }
export function setMix(w: World, m: number): World { return withState(w, { mix: clamp(Math.round(m), 0, 15) }); }
export function setPeakHold(w: World, v: boolean): World { return withState(w, { peakHold: v }); }
export function setMuteEnable(w: World, v: boolean): World { return withState(w, { muteEnable: v }); }
export function setLocked(w: World, v: boolean): World { return withState(w, { locked: v }); }
export function unlock(w: World): World { return withState(w, { locked: false }); }
export function setSceneConfirm(w: World, v: boolean): World { return withState(w, { sceneConfirm: v }); }

// ---------- scenes (confirm flow) ----------

export function pickScene(w: World, i: number): World {
  if (i < 0 || i >= scenes().length) return w;
  if (w.state.sceneConfirm) return withState(w, { scenePv: i });
  return withState(w, { scenePv: i, scene: i });
}

/** RECALL: enabled only when a preview differs; loads and returns to backTo. */
export function recallScene(w: World): World {
  if (w.state.scenePv === w.state.scene) return w;
  return withState(w, { scene: w.state.scenePv, mode: backTo(w) });
}

export interface SceneView {
  names: string[];
  curNum: string;
  curName: string;
  pvNum: string;
  pvName: string;
  pending: boolean;
  recallEnabled: boolean;
  recallTag: string;
}

export function sceneView(w: World): SceneView {
  const names = scenes();
  const pending = w.state.scenePv !== w.state.scene;
  return {
    names,
    curNum: String(w.state.scene + 1).padStart(3, '0'),
    curName: names[w.state.scene],
    pvNum: String(w.state.scenePv + 1).padStart(3, '0'),
    pvName: names[w.state.scenePv],
    pending,
    recallEnabled: pending,
    recallTag: pending ? 'Selected · not recalled yet' : 'Current scene',
  };
}

// ---------- selectors ----------

export function isChannel(s: UiState): boolean { return s.mode === 'channel'; }
export function isMixer(s: UiState): boolean { return s.mode === 'mixer'; }

export function showBack(s: UiState): boolean { return s.mode !== 'mixer'; }

export function pageTitle(s: UiState): string {
  return PAGE_TITLES[s.mode] ?? '';
}

export function backLabel(w: World): string {
  const to = backTo(w);
  return to === 'channel' ? (w.chans[w.state.sel]?.name ?? 'Channel') : 'Mixer';
}

export function currentChannel(w: World): Channel {
  return w.chans[w.state.sel];
}

/** Master strip id: SOF target mix, else Main LR (port of renderVals msId). */
export function masterId(w: World): string {
  return w.state.sof ? 'mix' + (w.state.mix + 1) : 'mainlr';
}

/** Strips-per-screen with the design's default of 12. */
export function visibleCount(spp: number | null, fallback = 12): number {
  return spp ?? fallback;
}

/** Port of the gridCols formula in renderVals. */
export function gridCols(spp: number | null, fallback = 12): string {
  const n = visibleCount(spp, fallback);
  return n >= 16
    ? 'repeat(16, minmax(64px, 1fr))'
    : 'repeat(16, calc((100% - ' + (3 * (n - 1)) + 'px) / ' + n + '))';
}

/** Bank pager for derived (phone/portrait) layouts; the 1280x720 design
 *  scrolls all 16. Pages are 0-based and clamped. */
export function pageCount(itemCount: number, spp: number): number {
  return Math.max(1, Math.ceil(itemCount / Math.max(1, spp)));
}

export function stripsForPage<T>(items: T[], spp: number, page: number): T[] {
  const per = Math.max(1, spp);
  const p = clamp(Math.round(page), 0, pageCount(items.length, per) - 1);
  return items.slice(p * per, p * per + per);
}

export interface TabDef {
  key: Tab;
  label: string;
  note: string;
}

const TAB_LABELS: Array<[Tab, string]> = [
  ['input', 'Input'],
  ['eq', 'HPF + EQ'],
  ['gate', 'Gate'],
  ['comp', 'Compressor'],
  ['sends', 'Sends'],
];

/** Channel tab rail with the note line per tab (port of renderVals tabs). */
export function tabDefs(ch: Channel): TabDef[] {
  return TAB_LABELS.map(([key, label]) => ({ key, label, note: tabNote(ch, key) }));
}

export function tabNote(ch: Channel, key: Tab): string {
  if (key === 'input') return ch.input ? ch.src + (ch.input.phantom ? ' · 48V' : '') : 'Bus';
  if (key === 'eq') return ch.eq && ch.eq.on ? 'In' : 'Bypassed';
  if (key === 'gate') return ch.dyn && ch.dyn.gate.on ? 'In' : 'Off';
  if (key === 'comp') return ch.dyn && ch.dyn.comp.on ? 'In' : 'Off';
  return ch.sends ? '16 mixes' : 'None';
}

export function tabLabel(tab: Tab): string {
  return TAB_LABELS.find(t => t[0] === tab)?.[1] ?? tab;
}

export interface HeaderView {
  showBack: boolean;
  backLabel: string;
  isChannel: boolean;
  channelName: string;
  channelTabLabel: string;
  channelColor: string;
  showPageTitle: boolean;
  pageTitle: string;
  locked: boolean;
  connDot: string;
  connAria: string;
  curSceneNum: string;
  curSceneName: string;
  sceneAria: string;
  isScenes: boolean;
}

/** Top-bar state (port of the header section of renderVals). */
export function headerView(w: World): HeaderView {
  const s = w.state;
  const names = scenes();
  const offline = s.connected === 'offline';
  const online = !!s.connected && !offline;
  const ch = w.chans[s.sel];
  return {
    showBack: showBack(s),
    backLabel: backLabel(w),
    isChannel: isChannel(s),
    channelName: ch?.name ?? '',
    channelTabLabel: tabLabel(s.tab),
    channelColor: ch?.color ?? '',
    showPageTitle: !!PAGE_TITLES[s.mode],
    pageTitle: pageTitle(s),
    locked: s.locked,
    connDot: online ? '#5fe060' : offline ? '#f0b44a' : '#ff4a3d',
    connAria: online ? 'Connection: online, select device' : 'Connection: not connected, select device',
    curSceneNum: String(s.scene + 1).padStart(3, '0'),
    curSceneName: names[s.scene],
    sceneAria: 'Scene ' + (s.scene + 1) + ' ' + names[s.scene] + ', open scene list',
    isScenes: s.mode === 'scenes',
  };
}

// ---------- pure view-text builders (no callbacks; what screens read) ----------

export interface StripText {
  id: string;
  name: string;
  src: string;
  selectAria: string;
  eqAria: string;
  dynAria: string;
  hasPan: boolean;
  panText: string;
  panAria: string;
  readout: string;
  readColor: string;
  faderAria: string;
  muted: boolean;
}

/** Strip texts for a channel id (meter-dependent fields excluded). */
export function stripText(w: World, id: string): StripText | null {
  const c = w.chans[id];
  if (!c) return null;
  const useSend = w.state.sof && !!c.sends;
  const pos = useSend ? c.sends![w.state.mix] : c.fader;
  return {
    id, name: c.name, src: c.src,
    selectAria: 'Select ' + c.name + ', source ' + c.src,
    eqAria: 'Open EQ for ' + c.name,
    dynAria: 'Open dynamics for ' + c.name,
    hasPan: c.pan != null,
    panText: panText(c.pan),
    panAria: panAria(c.name, c.pan),
    readout: fmtDb(pos),
    readColor: useSend ? '#22d3e6' : '#c9d1dc',
    faderAria: faderAria(c.name, pos, useSend, w.state.mix),
    muted: c.mute,
  };
}

export interface SendRowText {
  name: string;
  text: string;
  aria: string;
  preLabel: string;
  pre: boolean;
}

export function sendRowsText(ch: Channel): SendRowText[] {
  if (!ch.sends) return [];
  return ch.sends.map((v, k) => ({
    name: 'Mix ' + (k + 1),
    text: fmtDb(v),
    aria: sendAria(k, v),
    preLabel: ch.sendPre![k] ? 'Pre' : 'Post',
    pre: ch.sendPre![k],
  }));
}

export interface EqCardText {
  name: string;
  freq: string;
  gain: string | null;
  q: string | null;
}

const EQ_NAMES = ['HPF', 'Low', 'Lo-Mid', 'Hi-Mid', 'High'];

export function eqCardsText(ch: Channel): EqCardText[] | null {
  if (!ch.eq) return null;
  const cards: EqCardText[] = [{ name: 'HPF', ...hpfTexts(ch.eq.hpf), gain: null, q: null }];
  ch.eq.bands.forEach((b, i) => {
    const t = eqBandTexts(b);
    cards.push({ name: EQ_NAMES[i + 1], freq: t.freq, gain: t.gain, q: t.q });
  });
  return cards;
}

export function selInfoText(w: World, ch: Channel): string {
  const sb = w.state.selBand;
  if (!ch.eq) return '';
  if (sb === 0) {
    const h = ch.eq.hpf;
    return 'HPF · ' + fmtF(h.f) + ' · ' + h.slope + ' dB/oct' + (h.on ? '' : ' · off');
  }
  const b = ch.eq.bands[sb - 1];
  if (!b) return '';
  return EQ_NAMES[sb] + ' · ' + fmtF(b.f) + ' · ' + fmtG(b.g) + ' · Q ' + b.q.toFixed(2) + (b.on ? '' : ' · off');
}

export function dynSummaryText(ch: Channel, which: 'gate' | 'comp'): string {
  if (!ch.dyn) return '';
  return which === 'gate' ? gateSummary(ch.dyn.gate) : compSummary(ch.dyn.comp);
}

export function inputTexts(ch: Channel): { gain: string; trim: string; delay: string; delayDist: string } | null {
  if (!ch.input) return null;
  const [gain, trim] = inputKnobTexts(ch.input);
  const [delay] = delayKnobTexts(ch.input);
  return { gain: gain.value, trim: trim.value, delay: delay.value, delayDist: delayDistanceText(ch.input.delay) };
}

export interface FxSlotText {
  name: string;
  type: string;
  state: string;
  ret: string;
}

export function fxSlotsText(fx: FxSlot[]): FxSlotText[] {
  return fx.map((s, k) => ({
    name: 'FX ' + (k + 1),
    type: s.type,
    state: fxStateLabel(s),
    ret: fxRetLabel(k),
  }));
}

export function fxTitleText(fxSel: number, slot: FxSlot): string {
  return 'FX ' + (fxSel + 1) + ' · ' + slot.type;
}

export function fxKnobsText(slot: FxSlot): Array<{ label: string; value: string }> {
  return fxKnobTexts(slot).map(k => ({ label: k.label, value: k.value }));
}

export function inputLevelText(levelDb: number): string {
  return levelDb < -89 ? '-∞ dBFS' : levelDb.toFixed(1) + ' dBFS';
}

export { clamp as clampValue, toN as specToN, fromN as specFromN, fmtMs as fmtMsValue };

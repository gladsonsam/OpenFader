// D2 State package: transition tests for the pure uiLogic state machine.
// Run: node --experimental-strip-types apps/web/src/ui/state/uiLogic.test.ts

import { strict as assert } from 'node:assert';
import {
  backLabel,
  backTo,
  channelOrder,
  connectDevice,
  currentChannel,
  disconnect,
  dynSpecKey,
  eqNodeFromPos,
  eqParamFromDrag,
  expirePhantom,
  faderFromDrag,
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
  masterId,
  openDyn,
  openEq,
  openPage,
  openScenes,
  pageCount,
  panFromDrag,
  patchSrc,
  pickScene,
  recallScene,
  resetEqParam,
  routeBankIds,
  scanDevices,
  sceneView,
  selectChannel,
  sendFromDrag,
  setBank,
  setFader,
  setFine,
  setFxParam,
  setFxSel,
  setFxType,
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
  setSpp,
  setTab,
  setDynParam,
  setEqBandType,
  setEqParam,
  setHpfSlope,
  specFromDrag,
  srcTileLabels,
  stepChannel,
  stripText,
  stripsForPage,
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
  nextPatchSel,
  phantomLabel,
  sendRowsText,
  eqCardsText,
  selInfoText,
  dynSummaryText,
  inputTexts,
  fxSlotsText,
  fxKnobsText,
} from './uiLogic.ts';
import {
  initialStateFromUrl,
  markReady,
  modeTabToScreen,
  parseFixtureUrl,
  screenToModeTab,
  stateToSearch,
} from './urlBridge.ts';
import { MeterSim, meterIdsFor } from '../fixtures/meterSim.ts';
import { tapUpdate } from '../fixtures/designData.ts';

let n = 0;
function ok(cond: unknown, label: string): void {
  n++;
  assert.ok(cond, label);
}
function eq<T>(a: T, b: T, label: string): void {
  n++;
  assert.deepStrictEqual(a, b, `${label}: ${JSON.stringify(a)} !== ${JSON.stringify(b)}`);
}

// ---------- navigation / back stack ----------
{
  let w = initialWorld();
  eq(w.state.mode, 'mixer', 'starts on mixer');
  w = openEq(w, 'in2');
  eq(w.state.mode, 'channel', 'openEq -> channel');
  eq(w.state.tab, 'eq', 'openEq -> eq tab');
  eq(w.state.sel, 'in2', 'openEq selects');
  eq(backTo(w), 'mixer', 'back from channel');
  eq(backLabel(w), 'Mixer', 'back label mixer');
  w = goFx(w);
  eq(w.state.mode, 'fx', 'goFx');
  eq(w.state.prevMode, 'channel', 'prevMode kept');
  eq(backLabel(w), 'Ch 2', 'back label ch2');
  w = goBack(w);
  eq(w.state.mode, 'channel', 'back fx->channel');
  eq(backLabel(w), 'Mixer', 'back label mixer on channel');
  // mixer -> fx -> back -> mixer
  let m = initialWorld();
  m = goFx(m);
  m = goBack(m);
  eq(m.state.mode, 'mixer', 'back fx->mixer');
  // pages preserve prevMode across page hops: mixer->scenes->fx->back->mixer
  let p = initialWorld();
  p = openScenes(p);
  eq(p.state.prevMode, 'mixer', 'scenes prev');
  p = goFx(p);
  eq(p.state.prevMode, 'mixer', 'fx prev stays mixer');
  p = goBack(p);
  eq(p.state.mode, 'mixer', 'back fx->mixer via scenes');
  ok(headerView(p).showBack === false, 'no back on mixer');
  ok(headerView(openEq(initialWorld(), 'in2')).showBack, 'back shown on channel');
  eq(headerView(goRouting(initialWorld())).pageTitle, 'Routing', 'page title');
  eq(headerView(goConnect(initialWorld())).connDot, '#5fe060', 'conn dot online');
  eq(headerView(goOffline(initialWorld())).connDot, '#f0b44a', 'conn dot offline');
  // select + step
  let s = selectChannel(initialWorld(), 'in5');
  eq(currentChannel(s).name, 'Ch 5', 'select');
  s = stepChannel(s, 1);
  eq(s.state.sel, 'in6', 'step +1');
  s = stepChannel(s, -1);
  eq(s.state.sel, 'in5', 'step -1');
  s = setTab(setSelBand(s, 3), 'gate');
  eq([s.state.tab, s.state.selBand].join(), 'gate,3', 'tab+selband');
  s = setPeakHold(s, false);
  eq(s.state.peakHold, false, 'peakhold');
  eq(channelOrder().length, 48 + 16 + 16 + 16 + 10, 'channel order len');
  eq(headerView(goSettings(initialWorld())).pageTitle, 'Settings', 'settings title');
  eq(openPage(initialWorld(), 'routing').state.prevMode, 'mixer', 'openPage prev');
  // openDyn selects comp tab
  const d = openDyn(initialWorld(), 'in3');
  eq([d.state.mode, d.state.tab, d.state.sel].join(), 'channel,comp,in3', 'openDyn');
  // goPatch
  const g = goPatch(initialWorld(), 'in20');
  eq([g.state.mode, g.state.patchSel, g.state.routeBank].join(), 'routing,in20,1', 'goPatch');
}

// ---------- scenes confirm flow ----------
{
  let w = initialWorld(); // scene=4, scenePv=4
  w = openScenes(w);
  eq(w.state.mode, 'scenes', 'openScenes mode');
  eq(w.state.scenePv, 4, 'scenePv reset on open');
  w = pickScene(w, 7);
  eq(w.state.scenePv, 7, 'pick sets preview');
  eq(w.state.scene, 4, 'pick keeps scene in confirm mode');
  let v = sceneView(w);
  eq(v.pending, true, 'pending');
  eq(v.recallEnabled, true, 'recall enabled');
  eq(v.pvName, 'Set 2', 'pv name');
  w = recallScene(w);
  eq(w.state.scene, 7, 'recall loads');
  eq(w.state.mode, 'mixer', 'recall returns to backTo');
  // recall with nothing pending is a no-op
  const same = recallScene(w);
  eq(same.state.scene, 7, 'recall no-op');
  // instant mode
  let i = setSceneConfirm(initialWorld(), false);
  i = pickScene(i, 2);
  eq([i.state.scene, i.state.scenePv].join(), '2,2', 'instant loads');
  eq(sceneView(i).pending, false, 'not pending');
  eq(sceneView(i).recallTag, 'Current scene', 'recall tag');
}

// ---------- lock blocks edits, not navigation ----------
{
  let w = setLocked(initialWorld(), true);
  const f0 = w.chans['in1'].fader;
  w = setFader(w, 'in1', 0.1);
  eq(w.chans['in1'].fader, f0, 'lock blocks fader');
  w = toggleMute(w, 'in1');
  eq(w.chans['in1'].mute, false, 'lock blocks mute');
  w = setPan(w, 'in2', 0.5);
  eq(w.chans['in2'].pan !== 0.5, true, 'lock blocks pan');
  w = setSend(w, 'in2', 0, 0.9);
  ok(w.chans['in2'].sends![0] !== 0.9, 'lock blocks send');
  w = toggleSendPre(w, 'in2', 0);
  eq(w.chans['in2'].sendPre![0], true, 'lock blocks pre toggle');
  w = toggleDyn(w, 'in2', 'comp');
  const compOn = initialWorld().chans['in2'].dyn!.comp.on;
  eq(w.chans['in2'].dyn!.comp.on, compOn, 'lock blocks dyn toggle');
  w = toggleDelayOn(w, 'in2');
  eq(w.chans['in2'].input!.delayOn, false, 'lock blocks delay');
  w = toggleInvert(w, 'in2');
  eq(w.chans['in2'].input!.invert, false, 'lock blocks invert');
  w = togglePhantom(w, 'in2');
  eq(w.state.phantomArm, null, 'lock blocks phantom');
  w = patchSrc(w, 'Lcl 05');
  eq(w.chans['in1'].src, 'Lcl 01', 'lock blocks patch');
  w = unpatch(w);
  eq(w.chans['in1'].src, 'Lcl 01', 'lock blocks unpatch');
  // navigation + eq edits still work
  w = setBank(w, 'mix');
  eq(w.state.bank, 'mix', 'lock allows bank');
  w = openEq(w, 'mix1');
  eq(w.state.sel, 'mix1', 'lock allows select');
  w = setEqParam(w, 'mix1', 1, 'g', 3);
  eq(w.chans['mix1'].eq!.bands[0].g, 3, 'lock allows eq edit');
  w = pickScene(w, 1);
  eq(w.state.scenePv, 1, 'lock allows scene pick');
  w = unlock(w);
  w = toggleMute(w, 'in1');
  eq(w.chans['in1'].mute, true, 'unlock allows mute');
  // muteEnable off blocks mute even unlocked
  let m = setMuteEnable(initialWorld(), false);
  m = toggleMute(m, 'in1');
  eq(m.chans['in1'].mute, false, 'muteEnable blocks mute');
}

// ---------- phantom arm flow (injected clock via explicit expiry) ----------
{
  let w = initialWorld();
  w = togglePhantom(w, 'in2');
  eq(w.state.phantomArm, 'in2', 'phantom armed');
  eq(phantomLabel(w, w.chans['in2']), 'Tap again: 48V on', 'armed label off->on');
  w = togglePhantom(w, 'in2');
  eq(w.chans['in2'].input!.phantom, true, 'second tap commits');
  eq(w.state.phantomArm, null, 'arm cleared');
  eq(phantomLabel(w, w.chans['in2']), '48V On', 'on label');
  // expiry path
  w = togglePhantom(w, 'in3');
  w = expirePhantom(w, 'in4');
  eq(w.state.phantomArm, 'in3', 'expiry only matching id');
  w = expirePhantom(w, 'in3');
  eq(w.state.phantomArm, null, 'expired');
}

// ---------- SOF / mix target ----------
{
  let w = initialWorld();
  eq(masterId(w), 'mainlr', 'master default');
  w = setSof(w, true);
  eq(masterId(w), 'mix1', 'sof master mix1');
  w = setMix(w, 4);
  eq(masterId(w), 'mix5', 'mix target');
  const st = stripText(w, 'in2')!;
  eq(st.faderAria.startsWith('Send to Mix 5 from Ch 2'), true, 'sof fader aria');
  eq(st.readColor, '#22d3e6', 'sof read color');
  const w2 = setSof(initialWorld(), false);
  eq(stripText(w2, 'in2')!.faderAria.startsWith('Fader Ch 2'), true, 'fader aria');
}

// ---------- paging / spp ----------
{
  eq(visibleCount(null), 12, 'spp default');
  eq(visibleCount(8), 8, 'spp 8');
  eq(gridCols(null).includes('/ 12)'), true, 'grid 12');
  eq(gridCols(8).includes('/ 8)'), true, 'grid 8');
  eq(gridCols(16), 'repeat(16, minmax(80px, min(150px, calc((100% - 45px) / 16))))', 'grid 16');
  eq(pageCount(16, 8), 2, 'pages 16/8');
  eq(pageCount(16, 12), 2, 'pages 16/12');
  eq(pageCount(10, 16), 1, 'pages 10/16');
  const items = Array.from({ length: 16 }, (_, i) => i);
  eq(stripsForPage(items, 8, 0).length, 8, 'page0 len');
  eq(stripsForPage(items, 8, 1)[0], 8, 'page1 start');
  eq(stripsForPage(items, 12, 5).length, 4, 'page clamp');
  let w = setSpp(initialWorld(), 8);
  eq(w.state.spp, 8, 'setSpp');
  w = setFine(w, true);
  eq([w.state.fine, faderFromDrag(0.5, -10, 100, true)].join(), 'true,0.475', 'fine fader drag');
  eq(faderFromDrag(0.5, -10, 100, false), 0.4, 'coarse fader drag');
  eq(panFromDrag(0, 100, 200, false), 1, 'pan drag clamp');
  eq(sendFromDrag(0.2, 10, 100, true), 0.225, 'send drag fine');
  eq(eqParamFromDrag('f', 1, 496, -30, false), 248, 'eq freq octave down');
  eq(eqParamFromDrag('g', 1, 0, 10, false), 1, 'eq gain drag');
  const nd = eqNodeFromPos(1, 0, 0.5);
  eq([Math.round(nd.f), nd.g].join(), '20,0', 'node pos');
  eq(specFromDrag('gain', 0.5, 45, false), 45, 'spec drag');
  eq(dynSpecKey('comp', 'ratio'), 'ratio', 'dyn key');
}

// ---------- dynamics / eq / fx edits ----------
{
  let w = initialWorld();
  w = setDynParam(w, 'in2', 'comp', 'thr', 1);
  eq(w.chans['in2'].dyn!.comp.thr, 0, 'dyn max via spec');
  w = toggleDyn(w, 'in2', 'gate');
  eq(w.chans['in2'].dyn!.gate.on, !initialWorld().chans['in2'].dyn!.gate.on, 'dyn toggle');
  w = resetEqParam(w, 'in2', 1, 'g');
  eq(w.chans['in2'].eq!.bands[0].g, 0, 'reset gain');
  w = setEqBandType(w, 'in2', 0, 'PEQ');
  eq(w.chans['in2'].eq!.bands[0].type, 'PEQ', 'band type');
  w = setHpfSlope(w, 'in2', 24);
  eq(w.chans['in2'].eq!.hpf.slope, 24, 'hpf slope');
  w = toggleEqBand(w, 'in2', 0);
  eq(w.chans['in2'].eq!.hpf.on, !initialWorld().chans['in2'].eq!.hpf.on, 'hpf toggle');
  w = flatEq(w, 'in2');
  ok(w.chans['in2'].eq!.bands.every(b => b.g === 0), 'flat');
  w = toggleEqOn(w, 'in2');
  eq(w.chans['in2'].eq!.on, false, 'eq off');
  w = setFxSel(w, 3);
  eq(w.state.fxSel, 3, 'fxsel');
  w = setFxType(w, 3, 'Room');
  eq(w.fx[3].type, 'Room', 'fxtype');
  w = setFxParam(w, 3, 0, 0.9);
  eq(w.fx[3].p[0], 0.9, 'fxparam');
  eq(w.fx[3].bpm, null, 'fxparam clears bpm');
  w = toggleFxOn(w, 3);
  eq(w.fx[3].on, !initialWorld().fx[3].on, 'fxtoggle');
  // tap tempo with injected clock
  let taps: number[] = [];
  let r = tapFx(w, 0, taps, 1000);
  taps = r.taps;
  eq(taps.length, 1, 'first tap');
  r = tapFx(r.world, 0, taps, 1500);
  eq(r.world.fx[0].bpm, 120, 'tap bpm');
  ok(Math.abs(r.world.fx[0].p[0] - Math.log(25) / Math.log(100)) < 1e-12, 'tap p0');
  // tapUpdate unit: stale taps filtered
  const t = tapUpdate([0, 100], 5000);
  eq(t.bpm, null, 'stale taps expire');
}

// ---------- routing / connection ----------
{
  let w = initialWorld();
  w = setPatchSel(w, 'in5');
  eq(w.state.patchSel, 'in5', 'patchsel');
  w = setPatchGroup(w, 'Net');
  eq(w.state.patchGroup, 'Net', 'patchgroup');
  w = setRouteBank(w, 2);
  eq(w.state.routeBank, 2, 'routebank');
  eq(routeBankIds(0)[0], 'in1', 'routebank ids');
  eq(routeBankIds(2)[15], 'in48', 'routebank ids end');
  eq(srcTileLabels('Lcl')[0], 'Lcl 01', 'src labels');
  eq(nextPatchSel('in48').patchSel, 'in1', 'patch wrap');
  eq(nextPatchSel('in48').routeBank, 0, 'patch wrap bank');
  w = patchSrc(initialWorld(), 'Lcl 05');
  eq(w.chans['in1'].src, 'Lcl 05', 'patch applies');
  eq(w.state.patchSel, 'in2', 'patch advances');
  w = unpatch(patchSrc(initialWorld(), 'Lcl 05'));
  eq(w.chans['in2'].src, '—', 'unpatch');
  w = scanDevices(w, '12:00');
  eq(w.state.lastScan, 'Scanned at 12:00', 'scan');
  w = disconnect(w);
  eq(w.state.connected, null, 'disconnect');
  w = connectDevice(w, 'mon');
  eq(w.state.connected, 'mon', 'connect');
  w = goOffline(w);
  eq(w.state.connected, 'offline', 'offline');
}

// ---------- selectors / view texts ----------
{
  const w0 = initialWorld();
  eq(tabDefs(w0.chans['in2']).map(t => t.key).join(), 'input,eq,gate,comp,sends', 'tabs');
  ok(tabDefs(w0.chans['in2']).find(t => t.key === 'eq')!.note === 'In', 'eq note');
  eq(selInfoText(w0, w0.chans['in2']), 'Low · 120 Hz · +0.5 dB · Q 0.70', 'selinfo');
  const chIn = w0.chans['in2'];
  eq(eqCardsText(chIn)![1].freq, '120 Hz', 'eq card');
  ok(dynSummaryText(chIn, 'comp').includes('3.0:1'), 'dyn summary');
  const it = inputTexts(chIn)!;
  eq([it.gain, it.trim].join(), '29.0 dB,0.0 dB', 'input texts');
  eq(fxSlotsText(w0.fx)[7].state, 'Bypassed', 'fx7 bypassed');
  eq(fxKnobsText(w0.fx[0])[0].value, '4.3 s', 'fx knob');
  const rows = sendRowsText(chIn);
  eq(rows.length, 16, 'send rows');
  eq(rows[8].pre, false, 'send9 post');
  const h = headerView(w0);
  eq([h.curSceneNum, h.curSceneName].join(' '), '005 Changeover', 'header scene');
  ok(h.sceneAria.startsWith('Scene 5'), 'scene aria');
  eq(resetPan(setPan(w0, 'in2', 0.5), 'in2').chans['in2'].pan, 0, 'pan reset');
  let b = setBank(w0, 'dca');
  eq(stripText(b, 'dca1')!.hasPan, false, 'dca no pan');
}

// ---------- url bridge ----------
{
  const p = parseFixtureUrl('?ui=4&fixture=1&freeze=1&screen=ch-comp&ch=2&bank=ch1&locked=1&sof=1&fine=1');
  eq(p.screen, 'ch-comp', 'parse screen');
  eq([p.ch, p.bank, p.locked, p.sof, p.fine].join(), '2,ch1,true,true,true', 'parse flags');
  eq(screenToModeTab('ch-gate').tab, 'gate', 'screen tab');
  eq(screenToModeTab('connection').mode, 'connect', 'connection mode');
  eq(modeTabToScreen('connect', 'eq'), 'connection', 'mode to screen');
  eq(modeTabToScreen('channel', 'sends'), 'ch-sends', 'mode to ch screen');
  const w0 = initialWorld();
  const r = initialStateFromUrl('?screen=ch-eq&ch=2', w0.state, w0.chans);
  eq([r.state.mode, r.state.tab, r.state.sel].join(), 'channel,eq,in2', 'initial from url');
  eq(r.fixture, false, 'fixture default');
  const bad = initialStateFromUrl('?screen=bogus&ch=99&bank=bogus', w0.state, w0.chans);
  eq([bad.state.mode, bad.state.sel, bad.state.bank].join(), 'mixer,in1,ch1', 'bad params ignored');
  const rt = parseFixtureUrl(stateToSearch({ ...w0.state, mode: 'channel', tab: 'comp', sel: 'in2' }));
  eq([rt.screen, rt.ch].join(), 'ch-comp,2', 'round trip');
  (globalThis as Record<string, unknown>).window = {};
  markReady();
  eq(((globalThis as Record<string, unknown>).window as Record<string, unknown>).__uiReady, true, 'markReady');
  delete (globalThis as Record<string, unknown>).window;
}

// ---------- meter sim ----------
{
  const ids = meterIdsFor(
    Array.from({ length: 16 }, (_, i) => 'in' + (i + 1)),
    'in2',
    0,
  );
  ok(ids.includes('mainlr') && ids.includes('mix1') && ids.includes('fx15'), 'meter ids');
  eq(new Set(ids).size, ids.length, 'meter ids unique');
  const w = initialWorld();
  const mk = () => {
    const m = new MeterSim(ids);
    m.bind(id => w.chans[id]?.act ?? 0, id => w.chans[id]?.ph ?? 0);
    return m;
  };
  // deterministic with seeded rand
  const seq = (s: number) => () => {
    s = (s * 16807) % 2147483647;
    return (s - 1) / 2147483646;
  };
  const a = mk(), b = mk();
  for (let t = 0; t < 5; t++) {
    a.step(t * 0.08, 1000 + t * 80, seq(42));
  }
  const c = mk();
  for (let t = 0; t < 5; t++) {
    c.step(t * 0.08, 1000 + t * 80, seq(42));
  }
  assert.deepStrictEqual([...a.level], [...c.level], 'seeded determinism');
  void b;
  // decay to floor when nothing is active
  const z = new MeterSim(['x']);
  z.bind(() => 0, () => 0);
  z.step(0, 0, () => 0.5);
  eq(z.levelOf('x'), -90, 'inactive floor');
  eq(z.clipping('x', 100), false, 'no clip');
  // clip + peak latch with hot activity
  const hot = new MeterSim(['h']);
  hot.bind(() => 1, () => 0);
  const tHot = Math.PI / 2 / 3.3;
  for (let i = 0; i < 30; i++) hot.step(tHot, 1000 + i, () => 0.999);
  ok(hot.levelOf('h') > -0.3, 'hot level');
  ok(hot.clipping('h', 1001), 'clip latched');
  ok(!hot.clipping('h', 1029 + 1501), 'clip expires after 1.5s');
  eq(hot.peakOf('h'), hot.levelOf('h'), 'peak follows max');
  // frozen mode: stable snapshot, no randomness
  const f = mk();
  f.freeze();
  const snap = [...f.level];
  f.step(9.99, 99999, () => 0.999);
  f.freeze();
  assert.deepStrictEqual([...f.level], snap, 'frozen stable');
  ok(snap.every(v => v <= 10 && v >= -90), 'frozen range');
  eq(MeterSim.coverOf(-90, d => d), 1, 'cover floor');
}

console.log(`\nlogic: ${n} assertions — all pass`);

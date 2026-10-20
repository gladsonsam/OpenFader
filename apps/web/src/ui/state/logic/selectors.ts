// uiLogic/selectors: derived selectors + pure view-text builders screens read.
// Pure, rune-free logic split out of state/uiLogic.ts (one version of
// the logic lives here; uiLogic.ts re-exports it). DOM-free, node-testable.

import {
  clamp,
  compSummary,
  delayDistanceText,
  delayKnobTexts,
  eqBandTexts,
  faderAria,
  fmtDb,
  fmtF,
  fmtG,
  fxKnobTexts,
  fxRetLabel,
  fxStateLabel,
  gateSummary,
  hpfTexts,
  inputKnobTexts,
  panAria,
  panText,
  scenes,
  sendAria,
  type Channel,
  type FxSlot,
} from '../../fixtures/designData.ts';
import { backTo } from './nav.ts';
import { isChannel, PAGE_TITLES, type Tab, type UiState, type World } from './types.ts';

// ---------- selectors ----------

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

/** Port of the gridCols formula in renderVals, with a desktop max strip
 *  width: strips never stretch fatter than the 150 px tablet strip — wider
 *  viewports fit more channels and scroll instead of inflating each strip.
 *  The 80 px floor keeps narrow desktop windows near 12 readable strips
 *  instead of squeezing in extra skinny ones. */
export const STRIP_MAX_PX = 150;
/** Desktop minimum strip width (tracks never shrink below this). */
export const STRIP_MIN_PX = 80;

export function gridCols(spp: number | null, fallback = 12, count = 16): string {
  const n = visibleCount(spp, fallback);
  const per = 'calc((100% - ' + (3 * (n - 1)) + 'px) / ' + n + ')';
  return 'repeat(' + Math.max(16, count) + ', minmax(' + STRIP_MIN_PX + 'px, min(' + STRIP_MAX_PX + 'px, ' + per + ')))';
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
  if (key === 'eq') return ch.eq?.on === null ? 'Unknown' : ch.eq?.on ? 'In' : 'Bypassed';
  if (key === 'gate') return ch.dyn?.gate.live && !ch.dyn.gate.live.supported.on ? 'Unavailable' : ch.dyn?.gate.live && !ch.dyn.gate.live.known.on ? 'Unknown' : ch.dyn && ch.dyn.gate.on ? 'In' : 'Off';
  if (key === 'comp') return ch.dyn?.comp.live && !ch.dyn.comp.live.supported.on ? 'Unavailable' : ch.dyn?.comp.live && !ch.dyn.comp.live.known.on ? 'Unknown' : ch.dyn && ch.dyn.comp.on ? 'In' : 'Off';
  return ch.sends ? `${ch.sends.length} sends` : 'None';
}

export function tabLabel(tab: Tab): string {
  // 'groups' is a phone-only rail entry (built in channelScope, not
  // TAB_LABELS, so the desktop rail data stays 5 tabs pixel-identical).
  if (tab === 'groups') return 'Groups';
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
    readout: fmtDb(pos, c.live?.driver),
    readColor: useSend ? 'var(--u4-cyan)' : 'var(--u4-text-4)',
    faderAria: faderAria(c.name, pos, useSend, w.state.mix, c.live?.driver),
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
    text: fmtDb(v, ch.live?.driver),
    aria: sendAria(k, v, ch.live?.driver),
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
  if (!Number.isFinite(levelDb)) return 'Unavailable';
  return levelDb < -89 ? '-∞ dBFS' : levelDb.toFixed(1) + ' dBFS';
}

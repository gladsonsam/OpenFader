// W5b Channel package: pure view-models for the channel screens.
//
// The layout-rendered ChannelScreen binds these through the channel scope
// (channelScope.ts): tab rail entries, master strip vm, input vm, send
// rows. Everything derived from the world lives here so it is
// node-testable. Colours/px copy
// docs/design/src/design-template.html via design-logic.js.txt
// (stripVals / inputVals / sendVals / tab rail in renderVals).
//
// Tab extensibility (MODULES.md): the tab rail is data. CHANNEL_TAB_ORDER
// lists the tab ids; tabListEntries() resolves title + sub-label note per
// channel from uiLogic.tabNote. An extension adds an id here, renders its
// content via registerChannelTab(id, Component) (see tabRegistry.ts), and
// adds a manifest in manifests/c1-channel.ts.

import {
  delayDistanceText,
  fmtDb,
  inputKnobTexts,
  delayKnobTexts,
  panAria,
  panText,
  faderAria,
  sendAria,
  type Channel,
} from '../../fixtures/designData.ts';
import {
  currentChannel,
  inputLevelText,
  stripText,
  tabDefs,
  type Tab,
  type World,
} from '../../state/uiLogic.ts';

export type { Tab };

/** Tab ids in rail order. Extensions append ids + registerChannelTab. */
export const CHANNEL_TAB_ORDER: Tab[] = ['input', 'eq', 'gate', 'comp', 'sends'];

export interface TabListEntry {
  id: Tab;
  title: string;
  note: string;
  current: boolean;
  border: string;
  bg: string;
}

/** Tab rail entries with the design's selected (channel-colour border) state. */
export function tabListEntries(ch: Channel, tab: Tab): TabListEntry[] {
  return tabDefs(ch).map(t => ({
    id: t.key,
    title: t.label,
    note: t.note,
    current: t.key === tab,
    border: t.key === tab ? ch.color : 'var(--u4-btn-bg)',
    bg: t.key === tab ? '#363d48' : 'var(--u4-btn-bg)',
  }));
}

export interface MasterStripVm {
  name: string;
  color: string;
  text: string;
  src: string;
  hasPan: boolean;
  pan: number | null;
  panText: string;
  panAria: string;
  muted: boolean;
  muteEnabled: boolean;
  muteAria: string;
  readout: string;
  readColor: string;
  faderPos: number;
  faderAria: string;
  accent: 'chan' | 'sof';
  fine: boolean;
}

/** Right channel strip (port of stripVals for the selected channel). */
export function masterStripVm(w: World): MasterStripVm {
  const ch = currentChannel(w);
  const st = w.state;
  const useSend = st.sof && !!ch.sends;
  const pos = useSend ? ch.sends![st.mix] : ch.fader;
  const t = stripText(w, ch.id)!;
  return {
    name: ch.name,
    color: ch.color,
    text: ch.text,
    src: ch.src,
    hasPan: t.hasPan,
    pan: ch.pan,
    panText: panText(ch.pan),
    panAria: panAria(ch.name, ch.pan),
    muted: t.muted,
    muteEnabled: st.muteEnable && !st.locked,
    muteAria: 'Mute ' + ch.name,
    readout: t.readout,
    readColor: t.readColor,
    faderPos: pos,
    faderAria: faderAria(ch.name, pos, useSend, st.mix),
    accent: useSend ? 'sof' : 'chan',
    fine: st.fine,
  };
}

export interface InputVm {
  hasInput: boolean;
  gain: number;
  trim: number;
  delayMs: number;
  gainText: string;
  trimText: string;
  delayText: string;
  delayDist: string;
  delayOn: boolean;
  delayLabel: string;
  delayBg: string;
  delayFg: string;
  invert: boolean;
  invertBg: string;
  invertFg: string;
  phantomOn: boolean;
  armed: boolean;
  phantomLabel: string;
  phantomBg: string;
  phantomFg: string;
  phantomBorder: string;
  srcName: string;
  srcGroup: string;
  delayKnobColor: string;
}

/** Input tab values + toggle visuals (port of inputVals). */
export function inputVm(w: World): InputVm | null {
  const ch = currentChannel(w);
  const inp = ch.input;
  if (!inp) return { hasInput: false } as InputVm;
  const st = w.state;
  const armed = st.phantomArm === ch.id;
  const [gainT, trimT] = inputKnobTexts(inp);
  const [delayT] = delayKnobTexts(inp);
  const grp = ch.src.slice(0, 3);
  return {
    hasInput: true,
    gain: inp.gain,
    trim: inp.trim,
    delayMs: inp.delay,
    gainText: gainT.value,
    trimText: trimT.value,
    delayText: delayT.value,
    delayDist: delayDistanceText(inp.delay),
    delayOn: inp.delayOn,
    delayLabel: inp.delayOn ? 'On' : 'Off',
    delayBg: inp.delayOn ? '#5cdb5c' : 'var(--u4-btn-bg)',
    delayFg: inp.delayOn ? '#10240f' : 'var(--u4-text-4)',
    invert: inp.invert,
    invertBg: inp.invert ? 'var(--u4-text-1)' : 'var(--u4-btn-bg)',
    invertFg: inp.invert ? '#0b0f14' : 'var(--u4-text-4)',
    phantomOn: inp.phantom,
    armed,
    phantomLabel: armed
      ? inp.phantom
        ? 'Tap again: 48V off'
        : 'Tap again: 48V on'
      : inp.phantom
        ? '48V On'
        : '48V Off',
    phantomBg: armed ? '#f0b44a' : inp.phantom ? '#d92d22' : 'var(--u4-btn-bg)',
    phantomFg: armed ? '#241703' : inp.phantom ? '#ffffff' : 'var(--u4-text-4)',
    phantomBorder: armed ? '#f6c76a' : inp.phantom ? '#ff6a60' : 'var(--u4-line-ctl)',
    srcName: ch.src,
    srcGroup:
      grp === 'Lcl'
        ? 'Local input'
        : grp === 'Net'
          ? 'Network input'
          : grp === 'USB'
            ? 'USB input'
            : 'Unpatched',
    delayKnobColor: inp.delayOn ? '#8fb4ff' : '#4a5262',
  };
}

export interface SendRowVm {
  index: number;
  name: string;
  value: number;
  pct: string;
  text: string;
  aria: string;
  fill: string;
  pre: boolean;
  preLabel: string;
  preAria: string;
  preBg: string;
  preFg: string;
}

/** Sends tab rows (port of sendVals rows). */
export function sendRowsVm(ch: Channel): SendRowVm[] {
  if (!ch.sends) return [];
  return ch.sends.map((v, k) => {
    const pre = ch.sendPre![k];
    return {
      index: k,
      name: 'Mix ' + (k + 1),
      value: v,
      pct: (v * 100).toFixed(1),
      text: fmtDb(v),
      aria: sendAria(k, v),
      fill: pre ? 'var(--u4-text-1)' : '#22d3e6',
      pre,
      preLabel: pre ? 'Pre' : 'Post',
      preAria: 'Mix ' + (k + 1) + ' pre-fader',
      preBg: pre ? 'var(--u4-text-1)' : 'var(--u4-btn-bg)',
      preFg: pre ? '#0b0f14' : 'var(--u4-text-4)',
    };
  });
}

export { inputLevelText };

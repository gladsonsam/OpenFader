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
import { tapLabel } from '../../world/mapping/params.ts';

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
    border: t.key === tab ? `var(--u4-channel-tab-border, ${ch.color})` : 'var(--u4-btn-bg)',
    bg: t.key === tab ? 'var(--u4-channel-tab-bg, #363d48)' : 'var(--u4-btn-bg)',
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
    faderAria: faderAria(ch.name, pos, useSend, st.mix, ch.live?.driver),
    accent: useSend ? 'sof' : 'chan',
    fine: st.fine,
  };
}

export interface InputVm {
  live?: import('../../fixtures/designData.ts').LiveFields;
  delaySupported?: boolean;
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
    live: inp.live,
    delaySupported: !inp.live || inp.live.supported.delay || inp.live.supported.delayOn,
    gain: inp.gain,
    trim: inp.trim,
    delayMs: inp.delay,
    gainText: gainT.value,
    trimText: trimT.value,
    delayText: delayT.value,
    delayDist: Number.isFinite(inp.delay) ? delayDistanceText(inp.delay) : 'Unknown',
    delayOn: inp.delayOn,
    delayLabel: inp.live && !inp.live.known.delayOn ? 'Unknown' : inp.delayOn ? 'On' : 'Off',
    delayBg: inp.delayOn ? '#5cdb5c' : 'var(--u4-btn-bg)',
    delayFg: inp.delayOn ? '#10240f' : 'var(--u4-text-4)',
    invert: inp.invert,
    invertBg: inp.invert ? 'var(--u4-choice-on-bg, var(--u4-text-1))' : 'var(--u4-btn-bg)',
    invertFg: inp.invert ? 'var(--u4-seg-on-fg)' : 'var(--u4-text-4)',
    phantomOn: inp.phantom,
    armed,
    phantomLabel: inp.live && !inp.live.known.phantom ? '48V Unknown' : armed
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
      grp === 'Lcl' || grp === 'Loc'
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
  /** Desk send-tap enum (null = unknown; row hides the tap control). */
  tap: number | null;
  /** True when the desk exposes sendN_tap for this slot: the row offers
   *  the desk's legal tap choices instead of the fixture Pre/Post toggle. */
  tapSupported: boolean;
  /** Legal tap choices for this slot in desk order (see tapOptionsForSlot). */
  tapOptions: number[];
  tapLabel: string;
  tapAria: string;
  tapBg: string;
  tapFg: string;
  /** Desk send group-on (null = unknown; row hidden unless supported). */
  grpon: boolean | null;
  grponSupported: boolean;
  grponAria: string;
  grponBg: string;
  grponFg: string;
}

/** Live desk overlay for the sends tab (null = offline: GRP + tap hide).
 *
 *  `sendSlots` lists the desk's generic send slots in desk order (1-based);
 *  null means the desk has no `sendN_*` plane (e.g. DL32S, which addresses
 *  `auxN_`/`fxN_` instead) and the fixture rows render untouched.
 *  `destName` labels one 1-based slot (`Bus 1-6`, `FX 1-4` on xair); null
 *  falls back to the generic `Mix N` row name.
 *  `tap` reads the desk `sendN_tap` enum (`0..=5` IN/PREEQ/POSTEQ/PRE/POST/
 *  GRP); `tapSupported` is the caps gate per slot; `tapOptions` lists the
 *  slot's legal choices in desk order (bus slots take all six, FX slots
 *  refuse GRP — see tapOptionsForSlot). */
export interface DeskSendsOverlay {
  grpon(n: number): boolean | null;
  grponSupported(n: number): boolean;
  tap(n: number): number | null;
  tapSupported(n: number): boolean;
  tapOptions(n: number): number[];
  sendSlots(): number[] | null;
  destName(n: number): string | null;
}

/** Sends tab rows (port of sendVals rows). The desk group-on toggle rides
 *  an optional live overlay: offline (or a desk without the leaf) the GRP
 *  button hides and fixture baselines are untouched. Live on a desk with a
 *  `sendN_*` plane (XR18: 10 slots), the rows narrow to the desk's slots in
 *  desk order with the desk destination names — `index` stays the 0-based
 *  fixture send slot so writes still target the right send. */
export function sendRowsVm(ch: Channel, desk?: DeskSendsOverlay | null): SendRowVm[] {
  if (!ch.sends) return [];
  const slots = desk?.sendSlots() ?? null;
  const ns = slots ?? ch.sends.map((_, k) => k + 1);
  const out: SendRowVm[] = [];
  for (const n of ns) {
    const k = n - 1;
    if (!Number.isInteger(n) || k < 0 || k >= ch.sends.length) continue;
    const v = ch.sends[k];
    const known = Number.isFinite(v);
    if (typeof v !== 'number') continue;
    const pre = ch.sendPre![k];
    const name = desk?.destName(n) ?? 'Mix ' + n;
    const gSupported = desk?.grponSupported(n) ?? false;
    const g = gSupported ? (desk?.grpon(n) ?? null) : null;
    const tapOk = desk?.tapSupported(n) ?? false;
    const tap = tapOk ? (desk?.tap(n) ?? null) : null;
    const tapOpts = tapOk ? (desk?.tapOptions(n) ?? []) : [];
    const tapName = tap === null ? '?' : tapLabel(tap);
    out.push({
      index: k,
      name,
      value: v,
      pct: known ? (v * 100).toFixed(1) : '0',
      text: fmtDb(v, ch.live?.driver),
      aria: 'Send to ' + name + ' ' + fmtDb(v, ch.live?.driver) + ' dB',
      fill: pre ? 'var(--u4-text-1)' : '#22d3e6',
      pre,
      preLabel: pre ? 'Pre' : 'Post',
      preAria: name + ' pre-fader',
      preBg: pre ? 'var(--u4-choice-on-bg, var(--u4-text-1))' : 'var(--u4-btn-bg)',
      preFg: pre ? 'var(--u4-seg-on-fg)' : 'var(--u4-text-4)',
      tap,
      tapSupported: tapOk,
      tapOptions: tapOpts,
      tapLabel: tapName,
      tapAria:
        tap === null
          ? name + ' tap point, state unknown'
          : name + ' tap point ' + tapName + ' — activate for the next tap point',
      tapBg: tap === null ? 'var(--u4-btn-bg)' : '#e8edf5',
      tapFg: tap === null ? 'var(--u4-text-4)' : '#0b0f14',
      grpon: g,
      grponSupported: gSupported,
      grponAria: name + ' group on' + (g === null ? ', state unknown' : g ? ', grouped' : ', ungrouped'),
      grponBg: g === true ? '#e8edf5' : 'var(--u4-btn-bg)',
      grponFg: g === true ? '#0b0f14' : 'var(--u4-text-4)',
    });
  }
  return out;
}

export { inputLevelText };

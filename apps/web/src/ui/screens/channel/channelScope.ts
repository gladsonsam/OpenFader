// W5b channel scope: pure assembly of the LayoutRenderer expression scope
// for the Channel screens (channel + input/sends/eq/gate/comp layouts).
//
// Everything the default layouts bind or switch on lives here, derived from
// the world with the C1/C2/C3 view-models (channelModel/eqModel/dynModel):
// tab descriptor list, master strip vm, input vm, send rows, EQ graph/cards
// and dyn graph/knobs. Live, non-serialisable feeds (meter sources, the dyn
// level reader, frozen flag) are passed in by the screen and land in scope
// untouched, so layout `bind` can hand them to the hot-path primitives.
//
// Rune-free and DOM-free: node-testable with a fixture world + fakes.

import {
  inputLevelText,
  inputVm,
  masterStripVm,
  sendRowsVm,
  tabListEntries,
  type InputVm,
  type MasterStripVm,
  type SendRowVm,
  type TabListEntry,
} from './channelModel.ts';
import {
  DYN_COLOR,
  DYN_KNOBS,
  dynAria,
  dynAxis,
  dynCurvePath,
  dynDot,
  dynFillColor,
  dynFillPath,
  dynGrid,
  dynSummary,
  dynThreshold,
  dynToggle,
  type DynKind,
} from './dyn/dynModel.ts';
import {
  EQ_BAND_COLORS,
  eqBandCardsVm,
  eqGraphVm,
  eqHpfCardVm,
  eqMasterToggle,
  eqSelInfoText,
  type EqBandCardVm,
  type EqGraphVm,
  type EqHpfCardVm,
  type EqMasterToggle,
} from './eq/eqModel.ts';
import type { Channel } from '../fixtures/designData.ts';
import type { Tab, UiState, World } from '../state/uiLogic.ts';

export interface ChannelScopeFeeds {
  /** Live meter feed for the master strip fader (reads ui.meter). */
  stripMeter: unknown;
  /** Live meter feed for the input source hbar. */
  hbarMeter: unknown;
  /** Current input level in dBFS (formats the level text). */
  levelDb: number;
  /** Live level reader for the dyn dot/GR (null when frozen). */
  dynGetLevel: (() => number) | null;
  /** Fixture/live freeze flag (meters paint silence when true). */
  frozen: boolean;
}

export interface ChannelScopeArgs extends ChannelScopeFeeds {
  state: UiState;
  chans: Record<string, Channel>;
  /** FX plane (part of the World the strip vm reads). */
  fx: import('../state/uiLogic.ts').World['fx'];
  sel: Channel;
}

export interface ChannelScope {
  store: UiState;
  selId: string;
  tabs: TabListEntry[];
  master: MasterStripVm;
  inputVm: InputVm | null;
  levelText: string;
  sendRows: SendRowVm[];
  eqMaster: EqMasterToggle;
  eqGraph: EqGraphVm | null;
  eqSummary: string;
  eqHpf: EqHpfCardVm | null;
  eqBands: EqBandCardVm[];
  dynKind: DynKind;
  dynColor: string;
  dynAria: string;
  dynLabel: string;
  dynBg: string;
  dynFg: string;
  dynPressed: string;
  dynSummary: string;
  dynPath: string;
  dynFill: string;
  dynFillColor: string;
  dynThrX: string;
  dynThrLeft: string;
  dynThrText: string;
  dynDotL: string;
  dynDotT: string;
  dynGrid: ReturnType<typeof dynGrid>;
  dynAxis: ReturnType<typeof dynAxis>;
  dynParams: unknown;
  dynGate: unknown;
  dynComp: unknown;
  dynKnobs: (typeof DYN_KNOBS)[DynKind];
  lists: { tabs: TabListEntry[]; sends: SendRowVm[]; eqBands: EqBandCardVm[] };
  stripMeter: unknown;
  hbarMeter: unknown;
  dynGetLevel: (() => number) | null;
  frozen: boolean;
}

/** Tab the dyn modules render for (gate tab -> gate, everything else -> comp). */
export function dynKindForTab(tab: Tab): DynKind {
  return tab === 'gate' ? 'gate' : 'comp';
}

export function buildChannelScope(a: ChannelScopeArgs): ChannelScope {
  const world: World = { state: a.state, chans: a.chans, fx: a.fx };
  const ch = a.sel;
  const tab: Tab = a.state.tab;
  const tabs = tabListEntries(ch, tab);
  const sendRows = sendRowsVm(ch);
  const eq = ch.eq ?? null;
  const selBand = a.state.selBand;
  const eqGraph = eq ? eqGraphVm(eq, selBand) : null;
  const cards = eq ? eqBandCardsVm(eq, selBand, undefined, EQ_BAND_COLORS) : [];
  const gate = ch.dyn?.gate ?? null;
  const comp = ch.dyn?.comp ?? null;
  const kind = dynKindForTab(tab);
  const color = DYN_COLOR[kind];
  const params = kind === 'gate' ? gate : comp;
  const lo = kind === 'gate' ? -80 : -60;
  const thr = params ? dynThreshold(params, lo) : { x: '0', leftPct: '50', text: '' };
  const dot0 = params ? dynDot(kind, params, -90) : { leftPct: '0', topPct: '100' };
  const tgl = params
    ? dynToggle(kind, (params as { on: boolean }).on)
    : { bg: 'var(--u4-btn-bg)', fg: 'var(--u4-text-4)', label: '', pressed: 'false' };
  return {
    store: a.state,
    selId: ch.id,
    tabs,
    master: masterStripVm(world),
    inputVm: inputVm(world),
    levelText: inputLevelText(a.levelDb),
    sendRows,
    eqMaster: eq ? eqMasterToggle(eq) : { str: 'false', bg: 'var(--u4-btn-bg)', text: 'var(--u4-text-4)', label: 'EQ Bypassed' },
    eqGraph,
    eqSummary: eq ? eqSelInfoText(eq, selBand) : '',
    eqHpf: eq ? eqHpfCardVm(eq, selBand, EQ_BAND_COLORS) : null,
    eqBands: cards,
    dynKind: kind,
    dynColor: color,
    dynAria: dynAria(kind),
    dynLabel: tgl.label,
    dynBg: tgl.bg,
    dynFg: tgl.fg,
    dynPressed: tgl.pressed,
    dynSummary: gate && comp ? dynSummary(kind, gate, comp) : '',
    dynPath: params ? dynCurvePath(kind, params) : '',
    dynFill: params ? dynFillPath(kind, params) : '',
    dynFillColor: dynFillColor(kind),
    dynThrX: thr.x,
    dynThrLeft: thr.leftPct,
    dynThrText: thr.text,
    dynDotL: dot0.leftPct,
    dynDotT: dot0.topPct,
    dynGrid: dynGrid(lo),
    dynAxis: dynAxis(kind),
    dynParams: params,
    dynGate: gate,
    dynComp: comp,
    dynKnobs: DYN_KNOBS[kind],
    lists: { tabs, sends: sendRows, eqBands: cards },
    stripMeter: a.stripMeter,
    hbarMeter: a.hbarMeter,
    dynGetLevel: a.dynGetLevel,
    frozen: a.frozen,
  };
}

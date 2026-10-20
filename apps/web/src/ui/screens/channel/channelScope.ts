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
  type DeskSendsOverlay,
  type InputVm,
  type MasterStripVm,
  type SendRowVm,
  type TabListEntry,
} from './channelModel.ts';
import {
  DYN_COLOR,
  DYN_KNOBS,
  DYN_LO,
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
import type { Channel } from '../../fixtures/designData.ts';
import type { Tab, UiState, World } from '../../state/uiLogic.ts';
import type { GroupsVm, LiveGroupsSource } from './groupsModel.ts';
import { groupsVmFor } from './groupsModel.ts';
import type { DeskVm, LiveDeskSource } from './deskModel.ts';
import { deskVmFor } from './deskModel.ts';
import type { InsertVm, LiveInsertSource } from './insertModel.ts';
import { insertVmFor } from './insertModel.ts';

export interface ChannelScopeFeeds {
  /** Live meter feed for the master strip fader (reads ui.meter). */
  stripMeter: unknown;
  /** Live meter feed for the input source hbar. */
  hbarMeter: unknown;
  /** Current input level in dBFS (formats the level text). */
  levelDb: number;
  /** Live level reader for the dyn dot/GR (null when frozen). */
  dynGetLevel: (() => number) | null;
  dynGetReduction?: (kind: DynKind) => number | null;
  /** Fixture/live freeze flag (meters paint silence when true). */
  frozen: boolean;
}

export interface ChannelScopeArgs extends ChannelScopeFeeds {
  state: UiState;
  chans: Record<string, Channel>;
  /** FX plane (part of the World the strip vm reads). */
  fx: import('../../state/uiLogic.ts').World['fx'];
  sel: Channel;
  /** Live desk groups source (null offline: the card hides). */
  liveGroups?: LiveGroupsSource | null;
  /** Live desk-config source (null offline: the card hides). */
  liveDesk?: LiveDeskSource | null;
  /** Live insert source (null offline: the card hides). */
  liveInsert?: LiveInsertSource | null;
  /** Live desk send overlay (null offline: GRP buttons hide). */
  deskSends?: DeskSendsOverlay | null;
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
  /** Q of the selected parametric band (1..4) for the graph pinch gesture;
   *  0 when HPF (band 0) is selected or there is no EQ (HPF has no Q). */
  eqSelQ: number;
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
  /** Tab-independent gate transfer geometry for the read-only preview minis
   *  (bound regardless of tab so every tab can show the full chain). */
  gateAria: string;
  gatePath: string;
  gateFill: string;
  gateFillColor: string;
  gateThrX: string;
  gateThrLeft: string;
  gateThrText: string;
  gateDotL: string;
  gateDotT: string;
  gateGrid: ReturnType<typeof dynGrid>;
  gateAxis: ReturnType<typeof dynAxis>;
  /** Tab-independent comp transfer geometry (same preview purpose). */
  compAria: string;
  compPath: string;
  compFill: string;
  compFillColor: string;
  compThrX: string;
  compThrLeft: string;
  compThrText: string;
  compDotL: string;
  compDotT: string;
  compGrid: ReturnType<typeof dynGrid>;
  compAxis: ReturnType<typeof dynAxis>;
  dynParams: unknown;
  dynGate: unknown;
  dynComp: unknown;
  dynKnobs: (typeof DYN_KNOBS)[DynKind];
  lists: {
    tabs: TabListEntry[];
    sends: SendRowVm[];
    eqBands: EqBandCardVm[];
    /** Phone rail: the 5 desktop tabs plus the Groups tab (desktop and
     *  tablet keep `tabs`, so their rails stay pixel-identical). */
    phoneTabs: TabListEntry[];
  };
  stripMeter: unknown;
  hbarMeter: unknown;
  dynGetLevel: (() => number) | null;
  dynGetReduction?: (kind: DynKind) => number | null;
  frozen: boolean;
  /** Desk groups card vm (null offline: the layout `when` hides the card). */
  groupsVm: GroupsVm | null;
  /** Live GR reader for the assign card (null offline: static vm value). */
  assignGr: (() => number | null) | null;
  /** Desk-config card vm (null offline: the layout `when` hides the card). */
  deskVm: DeskVm | null;
  /** Insert card vm (null offline: the layout `when` hides the card). */
  insertVm: InsertVm | null;
}

/** Tab the dyn modules render for (gate tab -> gate, everything else -> comp). */
export function dynKindForTab(tab: Tab): DynKind {
  return tab === 'gate' ? 'gate' : 'comp';
}

/**
 * Phone rail note for the Groups tab (mute-master count, echo-honest).
 * The fixture channel carries no desk-group data, so this reads the live
 * groups vm: null (offline/unmapped) reports Offline, matching the tab
 * body which hides the card offline.
 */
export function groupsTabNote(groupsVm: GroupsVm | null): string {
  if (!groupsVm) return 'Offline';
  const rows = [...groupsVm.mg, ...groupsVm.sg];
  if (rows.length === 0) return 'No groups';
  const muted = rows.filter(r => r.masterMuted === true).length;
  if (muted > 0) return muted === 1 ? '1 muted' : `${muted} muted`;
  if (rows.some(r => r.masterMuted === null)) return 'Not confirmed';
  return 'All open';
}

export function buildChannelScope(a: ChannelScopeArgs): ChannelScope {
  const world: World = { state: a.state, chans: a.chans, fx: a.fx };
  const ch = a.sel;
  const tab: Tab = a.state.tab;
  const tabs = tabListEntries(ch, tab);
  const sendRows = sendRowsVm(ch, a.deskSends ?? null);
  const eq = ch.eq ?? null;
  const selBand = a.state.selBand;
  const eqGraph = eq ? eqGraphVm(eq, selBand) : null;
  const selQ = selBand > 0 && eq ? (eq.bands[selBand - 1]?.q ?? 0) : 0;
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
  // Per-kind geometry for the preview minis: computed for BOTH kinds on
  // every tab (unlike the dyn* singles above, which follow dynKindForTab),
  // so the layouts can bind gate + comp minis without a tab switch.
  const gateThr = gate ? dynThreshold(gate, DYN_LO.gate) : { x: '0', leftPct: '50', text: '' };
  const compThr = comp ? dynThreshold(comp, DYN_LO.comp) : { x: '0', leftPct: '50', text: '' };
  const gateDot0 = gate ? dynDot('gate', gate, -90) : { leftPct: '0', topPct: '100' };
  const compDot0 = comp ? dynDot('comp', comp, -90) : { leftPct: '0', topPct: '100' };
  const groups = groupsVmFor(a.liveGroups ?? null);
  // Phone-only 6th rail entry (desktop/tablet render `tabs`, untouched).
  // Selected visuals copy tabListEntries; the note is the mute-master
  // count so the rail is glanceable without opening the tab.
  const groupsEntry: TabListEntry = {
    id: 'groups',
    title: 'Assign',
    note: groupsTabNote(groups),
    current: tab === 'groups',
    border: tab === 'groups' ? ch.color : 'var(--u4-btn-bg)',
    bg: tab === 'groups' ? '#363d48' : 'var(--u4-btn-bg)',
  };
  return {
    store: a.state,
    selId: ch.id,
    tabs: [...tabs, groupsEntry],
    master: masterStripVm(world),
    inputVm: inputVm(world),
    levelText: inputLevelText(a.levelDb),
    sendRows,
    eqMaster: eq ? eqMasterToggle(eq) : { str: 'false', bg: 'var(--u4-btn-bg)', text: 'var(--u4-text-4)', label: 'EQ Bypassed' },
    eqGraph,
    eqSummary: eq ? eqSelInfoText(eq, selBand) : '',
    eqSelQ: selQ,
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
    gateAria: dynAria('gate'),
    gatePath: gate ? dynCurvePath('gate', gate) : '',
    gateFill: gate ? dynFillPath('gate', gate) : '',
    gateFillColor: dynFillColor('gate'),
    gateThrX: gateThr.x,
    gateThrLeft: gateThr.leftPct,
    gateThrText: gateThr.text,
    gateDotL: gateDot0.leftPct,
    gateDotT: gateDot0.topPct,
    gateGrid: dynGrid(DYN_LO.gate),
    gateAxis: dynAxis('gate'),
    compAria: dynAria('comp'),
    compPath: comp ? dynCurvePath('comp', comp) : '',
    compFill: comp ? dynFillPath('comp', comp) : '',
    compFillColor: dynFillColor('comp'),
    compThrX: compThr.x,
    compThrLeft: compThr.leftPct,
    compThrText: compThr.text,
    compDotL: compDot0.leftPct,
    compDotT: compDot0.topPct,
    compGrid: dynGrid(DYN_LO.comp),
    compAxis: dynAxis('comp'),
    dynParams: params,
    dynGate: gate,
    dynComp: comp,
    dynKnobs: DYN_KNOBS[kind],
    lists: { tabs: [...tabs, groupsEntry], sends: sendRows, eqBands: cards, phoneTabs: [...tabs, groupsEntry] },
    stripMeter: a.stripMeter,
    hbarMeter: a.hbarMeter,
    dynGetLevel: a.dynGetLevel,
    dynGetReduction: a.dynGetReduction,
    frozen: a.frozen,
    groupsVm: groups,
    assignGr: a.liveGroups?.grComp ?? null,
    deskVm: deskVmFor(a.liveDesk ?? null),
    insertVm: insertVmFor(a.liveInsert ?? null),
  };
}

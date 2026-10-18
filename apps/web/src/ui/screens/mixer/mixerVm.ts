// M1 Mixer screen: pure view-model builders over the D2 store data.
//
// Exact port of the mixer branch of the design's renderVals/stripVals
// (docs/design/src/design-logic.js.txt): 16 strip slots (bank items plus
// dark empty placeholders for short banks), the SOF/master selection, the
// selection ring, and every aria label / colour the template reads.
// Components stay presentational: MixerScreen binds these to uiStore state
// + chans and wires ui.do actions.
//
// Run: node --experimental-strip-types apps/web/src/ui/screens/mixer/mixerVm.test.ts

import {
  bankByKey,
  dynThumbPath,
  eqThumbPath,
  fmtDb,
  type Channel,
} from '../../fixtures/designData.ts';
import type { MeterSim } from '../../fixtures/meterSim.ts';
import type { MeterSource } from '../../primitives/signal/Meter.svelte';
import {
  masterId,
  stripText,
  visibleCount,
  type UiState,
  type World,
} from '../../state/uiLogic.ts';

/** Fixed slot count: the design always renders 16 grid cells. */
export const SLOT_COUNT = 16;

/** Live desk group/link state for one fixture strip (null = no live link:
 *  the layout `when` hides the badges and fixture mode is untouched). */
export interface DeskGroups {
  /** 0-based mute-group indices with a confirmed membership echo. */
  mg: number[];
  /** 0-based subgroup indices with a confirmed membership echo. */
  sg: number[];
  /** Master mute per listed group (parallel to mg/sg; null = not yet echoed). */
  mgMuted: Array<boolean | null>;
  sgMuted: Array<boolean | null>;
  /** Stereo-link leaf: true/false when confirmed, null when unknown or
   *  unsupported on the desk (xair: global chlink, no per-channel leaf). */
  link: boolean | null;
}

export interface StripVm {
  id: string;
  name: string;
  /** Name-bar sub line: source patch, or 'Stereo out' / 'Aux bus' for master. */
  sub: string;
  color: string;
  text: string;
  /** Strip icon name (null = none). Rendered in the name bar. */
  icon: string | null;
  /** Selection ring: `0 0 0 2px <color>` on the selected strip, else 'none'. */
  ring: string;
  pan: number | null;
  panLeft: number;
  panText: string;
  panAria: string;
  hasEq: boolean;
  eqPath: string;
  eqStroke: string;
  dynPath: string;
  eqAria: string;
  dynAria: string;
  muted: boolean;
  /** Design muteOpacity gate: muteEnable && !locked. */
  muteEnabled: boolean;
  solo: boolean;
  /** Same gate as mute (solo shares the muteEnable + lock rules). */
  soloEnabled: boolean;
  /** SOF: fader shows/sets the send to the target mix. */
  useSend: boolean;
  value: number;
  readout: string;
  readColor: string;
  faderAria: string;
  selectAria: string;
  fine: boolean;
  /** Live desk groups/link (null offline: badges hidden, fixtures untouched). */
  desk: DeskGroups | null;
}

function stubWorld(state: UiState, chans: Record<string, Channel>): World {
  return { state, chans, fx: [] };
}

/** Strip view-model for a channel id (null when the id is unknown). */
export function stripVm(
  state: UiState,
  chans: Record<string, Channel>,
  id: string,
): StripVm | null {
  const c = chans[id];
  if (!c) return null;
  const st = stripText(stubWorld(state, chans), id);
  if (!st) return null;
  const useSend = state.sof && !!c.sends;
  const value = useSend ? c.sends![state.mix] : c.fader;
  const hasEq = !!c.eq;
  return {
    id,
    name: c.name,
    sub: c.src,
    color: c.color,
    text: c.text,
    icon: c.icon ?? null,
    ring: id === state.sel ? 'inset 0 0 0 2px ' + c.color : 'none',
    pan: c.pan,
    panLeft: 50 + (c.pan ?? 0) * 44,
    panText: st.panText,
    panAria: st.panAria,
    hasEq,
    eqPath: hasEq ? eqThumbPath(c) : '',
    eqStroke: hasEq && c.eq!.on ? '#d9c24a' : '#4a5262',
    dynPath: hasEq ? dynThumbPath(c) : '',
    eqAria: st.eqAria,
    dynAria: st.dynAria,
    muted: c.mute,
    muteEnabled: state.muteEnable && !state.locked,
    solo: c.solo,
    soloEnabled: state.muteEnable && !state.locked,
    useSend,
    value,
    readout: fmtDb(value),
    readColor: useSend ? 'var(--u4-cyan)' : 'var(--u4-text-4)',
    faderAria: st.faderAria,
    selectAria: st.selectAria,
    fine: state.fine,
    desk: null,
  };
}

export interface MasterVm extends StripVm {
  /** 'SENDS MASTER' in SOF, else 'MASTER'. */
  tag: string;
  tagColor: string;
  /** 'Aux bus' in SOF, else 'Stereo out'. */
  kind: 'sof' | 'master';
  /** Bus selector button label (`MIX ▾ · <name>`). */
  busLabel: string;
}

/** Master strip view-model: the SOF target mix, else Main LR. */
export function masterVm(state: UiState, chans: Record<string, Channel>): MasterVm | null {
  const id = masterId({ state, chans, fx: [] });
  const base = stripVm(state, chans, id);
  if (!base) return null;
  const sof = state.sof;
  return {
    ...base,
    sub: sof ? 'Aux bus' : 'Stereo out',
    readout: base.readout,
    // The design hard-codes the master readout colour; the token keeps it readable on light themes.
    readColor: 'var(--u4-text-1)',
    tag: sof ? 'SENDS MASTER' : 'MASTER',
    tagColor: sof ? '#5fe3f0' : '#ffd25a',
    kind: sof ? 'sof' : 'master',
    busLabel: 'MIX ▾ · ' + base.name,
  };
}

/**
 * The 16 grid slots for a bank: item ids, then '' for short banks
 * (mtx has 10 items -> 6 dark empty placeholders, like the design).
 */
export function mixerSlots(bankKey: string): string[] {
  const items = bankByKey(bankKey).items;
  return Array.from({ length: SLOT_COUNT }, (_, i) => items[i]?.id ?? '');
}

/** Strips-per-screen with the design's default of 12 (settings override). */
export function stripsPerScreen(spp: number | null): number {
  return visibleCount(spp);
}

/** All 48 input channels in order, for the phone pager (no banks: the
 *  rail is hidden on phones and the snap pager scrolls the whole desk). */
export function mixerAllCells(
  state: UiState,
  chans: Record<string, Channel>,
  meter: MeterSim | null,
  frozen: boolean,
  liveDb?: (fixtureId: string) => number | null,
  deskOf?: (fixtureId: string) => DeskGroups | null,
): StripCell[] {
  const out: StripCell[] = [];
  for (let n = 1; n <= 48; n++) {
    const id = 'in' + n;
    const vm = stripVm(state, chans, id);
    if (!vm) continue;
    const desk = deskOf?.(id) ?? null;
    out.push({ ...vm, desk, meter: stripMeterSource(chans, meter, frozen, state.peakHold, id, liveDb) });
  }
  return out;
}

/**
 * Phone strip-grid columns (responsive `phone` variant in mixer.default.json).
 * Narrow 120px strips: fader cap, mute/solo and pan all fit; the 48 fixed
 * columns keep the whole desk a horizontally snapping pager the user swipes
 * through (no bank rail on phones).
 */
export function phoneGridCols(): string {
  return 'repeat(48, 120px)';
}

/**
 * Tablet strip-grid columns (responsive `tablet` variant). Four full 150 px
 * strips fill the ~618 px tablet-portrait scrollport (834 px viewport minus
 * shell padding, bank rail and master).
 */
export function tabletGridCols(): string {
  return 'repeat(16, 150px)';
}

// ---------- layout scope builders (W5c conversion) ----------
//
// The LayoutRenderer passes per-strip data through `bind` expressions over
// `item` (see ui/layouts/mixer.default.json). Meters stay imperative: each
// strip view-model carries its own MeterSource of closures over the shared
// MeterSim (the same getters MixerScreen built inline before). The meter sim
// steps underneath without touching reactive state, so the renderer does no
// work per animation frame (SPEC 2a).

/** A strip view-model with its imperative meter feed attached. */
export interface StripCell extends StripVm {
  meter: MeterSource;
}

/** Master view-model with its imperative meter feed attached. */
export interface MasterCell extends MasterVm {
  meter: MeterSource;
}

/**
 * Imperative meter feed for one strip id. In fixture freeze mode the design
 * reference shows silent meters (levels unset -> -90, no peaks/clips/GR),
 * so frozen feeds report silence; live mode reads the shared sim. When a
 * real relay link is up, `liveDb` (dB, like levelOf) wins over the sim.
 */
export function stripMeterSource(
  chans: Record<string, Channel>,
  meter: MeterSim | null,
  frozen: boolean,
  peakHold: boolean,
  id: string,
  liveDb?: (fixtureId: string) => number | null,
): MeterSource {
  return {
    getLevel: () => liveDb?.(id) ?? (frozen ? -90 : (meter?.levelOf(id) ?? -90)),
    getPeak: () => {
      if (frozen) return null;
      const v = meter?.peakOf(id) ?? null;
      return v == null || v <= -90 ? null : v;
    },
    getClip: () => (frozen ? false : (meter ? meter.clipping(id, Date.now()) : false)),
    grOn: () => {
      if (frozen) return false;
      const dyn = chans[id]?.dyn;
      if (!dyn || !dyn.comp.on) return false;
      return (meter?.levelOf(id) ?? -90) > dyn.comp.thr;
    },
    showPeak: peakHold,
  };
}

/** Dual-channel master meter feed (right channel 1.5 dB down, like the design). */
export function masterMeterSource(
  meter: MeterSim | null,
  frozen: boolean,
  id: string,
  liveDb?: (fixtureId: string) => number | null,
): MeterSource {
  return {
    getLevel: (ch: number) => {
      const lv = liveDb?.(id);
      if (lv != null) return ch === 0 ? lv : lv - 1.5;
      // Frozen fixtures paint silence on both channels (design reference).
      if (frozen) return -90;
      const lvl = meter?.levelOf(id) ?? -90;
      return ch === 0 ? lvl : lvl - 1.5;
    },
    showPeak: false,
  };
}

/**
 * The filled strip cells for a bank, in slot order (unknown ids dropped).
 * Short banks leave trailing slots empty; the screen derives the empty-slot
 * count as SLOT_COUNT - cells.length for the second repeat.
 */
export function mixerCells(
  state: UiState,
  chans: Record<string, Channel>,
  meter: MeterSim | null,
  frozen: boolean,
  liveDb?: (fixtureId: string) => number | null,
  deskOf?: (fixtureId: string) => DeskGroups | null,
): StripCell[] {
  const out: StripCell[] = [];
  for (const id of mixerSlots(state.bank)) {
    if (id === '') continue;
    const vm = stripVm(state, chans, id);
    if (!vm) continue;
    const desk = deskOf?.(id) ?? null;
    out.push({ ...vm, desk, meter: stripMeterSource(chans, meter, frozen, state.peakHold, id, liveDb) });
  }
  return out;
}

/** The master cell (SOF target mix, else Main LR), with its meter feed. */
export function mixerMasterCell(
  state: UiState,
  chans: Record<string, Channel>,
  meter: MeterSim | null,
  frozen: boolean,
  liveDb?: (fixtureId: string) => number | null,
  deskOf?: (fixtureId: string) => DeskGroups | null,
): MasterCell | null {
  const vm = masterVm(state, chans);
  if (!vm) return null;
  return { ...vm, desk: deskOf?.(vm.id) ?? null, meter: masterMeterSource(meter, frozen, vm.id, liveDb) };
}

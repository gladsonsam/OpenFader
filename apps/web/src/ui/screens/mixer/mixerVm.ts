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

export interface StripVm {
  id: string;
  name: string;
  /** Name-bar sub line: source patch, or 'Stereo out' / 'Aux bus' for master. */
  sub: string;
  color: string;
  text: string;
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
  /** SOF: fader shows/sets the send to the target mix. */
  useSend: boolean;
  value: number;
  readout: string;
  readColor: string;
  faderAria: string;
  selectAria: string;
  fine: boolean;
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
    ring: id === state.sel ? '0 0 0 2px ' + c.color : 'none',
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
    useSend,
    value,
    readout: fmtDb(value),
    readColor: useSend ? '#22d3e6' : '#c9d1dc',
    faderAria: st.faderAria,
    selectAria: st.selectAria,
    fine: state.fine,
  };
}

export interface MasterVm extends StripVm {
  /** 'SENDS MASTER' in SOF, else 'MASTER'. */
  tag: string;
  tagColor: string;
  /** 'Aux bus' in SOF, else 'Stereo out'. */
  kind: 'sof' | 'master';
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
    // The design hard-codes the master readout colour to #e8edf5.
    readColor: '#e8edf5',
    tag: sof ? 'SENDS MASTER' : 'MASTER',
    tagColor: sof ? '#5fe3f0' : '#ffd25a',
    kind: sof ? 'sof' : 'master',
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

/**
 * Phone strip-grid columns (responsive `phone` variant in mixer.default.json).
 * Two full 132 px strips fill the ~282 px phone scrollport (390 px viewport
 * minus shell padding, the 96 px phone master and gaps); the 16 fixed columns
 * make the bank a horizontally snapping pager the user swipes through.
 */
export function phoneGridCols(): string {
  return 'repeat(16, 132px)';
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
 * so frozen feeds report silence; live mode reads the shared sim.
 */
export function stripMeterSource(
  chans: Record<string, Channel>,
  meter: MeterSim | null,
  frozen: boolean,
  peakHold: boolean,
  id: string,
): MeterSource {
  return {
    getLevel: () => (frozen ? -90 : (meter?.levelOf(id) ?? -90)),
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
export function masterMeterSource(meter: MeterSim | null, frozen: boolean, id: string): MeterSource {
  return {
    getLevel: (ch: number) => {
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
): StripCell[] {
  const out: StripCell[] = [];
  for (const id of mixerSlots(state.bank)) {
    if (id === '') continue;
    const vm = stripVm(state, chans, id);
    if (!vm) continue;
    out.push({ ...vm, meter: stripMeterSource(chans, meter, frozen, state.peakHold, id) });
  }
  return out;
}

/** The master cell (SOF target mix, else Main LR), with its meter feed. */
export function mixerMasterCell(
  state: UiState,
  chans: Record<string, Channel>,
  meter: MeterSim | null,
  frozen: boolean,
): MasterCell | null {
  const vm = masterVm(state, chans);
  if (!vm) return null;
  return { ...vm, meter: masterMeterSource(meter, frozen, vm.id) };
}

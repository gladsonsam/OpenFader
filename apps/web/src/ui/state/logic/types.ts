// uiLogic/types: shared state types + immutable update helpers.
// Pure, rune-free logic split out of state/uiLogic.ts (one version of
// the logic lives here; uiLogic.ts re-exports it). DOM-free, node-testable.

import {
  buildChannels,
  buildFx,
  type Channel,
  type FxSlot,
} from '../../fixtures/designData.ts';
import type { ThemePresetId } from '../../modules/theme.ts';

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
  /** UI theme preset id (Settings > Display; design-dark is the default). */
  theme: ThemePresetId;
  sceneConfirm: boolean;
  phantomArm: string | null;
  fxSel: number;
  patchSel: string;
  patchGroup: RouteGroup;
  routeBank: number;
  /** Staged routing changes (channel id -> source, '—' = unpatch).
   * Every matrix tap and preset stages here; Apply commits, tap-away
   * cancels. Null = nothing staged. */
  pendingPatch: { cells: Record<string, string> } | null;
  /** Name-bar editor target channel id (null = closed). */
  nameEdit: string | null;
  /** Connected device key, 'offline', or null. */
  connected: string | null;
  lastScan: string;
  /** Bus selector popup over the mixer master (item 7; null = closed). */
  busPopup: boolean;
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
    spp: null, peakHold: true, locked: false, theme: 'design-dark', sceneConfirm: true, phantomArm: null,
    fxSel: 0, patchSel: 'in1', patchGroup: 'Lcl', routeBank: 0, pendingPatch: null, nameEdit: null,
    connected: 'foh', lastScan: 'Scanned at start-up', busPopup: false,
  };
}

export function isChannel(s: UiState): boolean { return s.mode === 'channel'; }
export function isMixer(s: UiState): boolean { return s.mode === 'mixer'; }

export function initialWorld(): World {
  return { state: initialUiState(), chans: buildChannels(), fx: buildFx() };
}

// ---------- immutable update helpers ----------

export function withState(w: World, patch: Partial<UiState>): World {
  return { state: { ...w.state, ...patch }, chans: w.chans, fx: w.fx };
}

export function withChannel(w: World, id: string, fn: (c: Channel) => void): World {
  const prev = w.chans[id];
  if (!prev) return w;
  const next: Channel = JSON.parse(JSON.stringify(prev));
  fn(next);
  return { state: w.state, chans: { ...w.chans, [id]: next }, fx: w.fx };
}

export function withFx(w: World, i: number, fn: (s: FxSlot) => void): World {
  if (i < 0 || i >= w.fx.length) return w;
  const fx = w.fx.map((x, k) => (k === i ? JSON.parse(JSON.stringify(x)) : x));
  fn(fx[i]);
  return { state: w.state, chans: w.chans, fx };
}

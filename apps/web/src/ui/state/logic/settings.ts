// uiLogic/settings: behaviour settings (spp/fine/SOF/meters/lock/scenes).
// Pure, rune-free logic split out of state/uiLogic.ts (one version of
// the logic lives here; uiLogic.ts re-exports it). DOM-free, node-testable.

import { clamp } from '../../fixtures/designData.ts';
import { PRESETS, type ThemePresetId } from '../../modules/theme.ts';
import { type World, withState } from './types.ts';

// ---------- settings ----------

export function setSpp(w: World, spp: number): World {
  return withState(w, { spp });
}

/** Switch the UI theme (unknown ids are ignored; design-dark is default). */
export function setTheme(w: World, theme: string): World {
  const id = (Object.keys(PRESETS) as ThemePresetId[]).includes(theme as ThemePresetId) ? (theme as ThemePresetId) : 'design-dark';
  return withState(w, { theme: id });
}

export function setFine(w: World, v: boolean): World { return withState(w, { fine: v }); }
export function setSof(w: World, v: boolean): World { return withState(w, { sof: v }); }
export function setMix(w: World, m: number): World { return withState(w, { mix: clamp(Math.round(m), 0, 15) }); }
export function setPeakHold(w: World, v: boolean): World { return withState(w, { peakHold: v }); }
export function setMuteEnable(w: World, v: boolean): World { return withState(w, { muteEnable: v }); }
export function setLocked(w: World, v: boolean): World { return withState(w, { locked: v }); }
export function unlock(w: World): World { return withState(w, { locked: false }); }
export function setSceneConfirm(w: World, v: boolean): World { return withState(w, { sceneConfirm: v }); }

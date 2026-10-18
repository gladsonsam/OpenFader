// S910 Settings screen: pure view-model builder + data-driven descriptors.
//
// Exact port of the `settingsVals` branch of the design's renderVals
// (docs/design/src/design-logic.js.txt): section titles, row labels,
// descriptions, option colours and pressed states. Components stay
// presentational: the screen binds this to uiStore state and wires ui.do
// actions. Nothing here touches the DOM or the store.
//
// The rows are DATA-DRIVEN: SETTING_DESCRIPTORS lists every setting
// {key,title,description,control} so users/extensions can add or reorder
// settings; the store field is the binding (see `bind`). The screen groups
// descriptors by panel in first-appearance order.
// Run: node --experimental-strip-types apps/web/src/ui/screens/settings/settingsVm.test.ts

import { visibleCount, type UiState } from '../../state/uiLogic.ts';
import { PRESETS, type ThemePresetId } from '../../modules/theme.ts';

export type SettingPanelId = 'Display' | 'Faders' | 'Safety' | 'About';
export type SettingControl = 'segmented' | 'toggle' | 'pads' | 'none';

export interface SettingDescriptor {
  /** Store field (or pseudo-field); the binding is `setting:<key>`. */
  key: string;
  panel: SettingPanelId;
  title: string;
  /** Static description; the Console row resolves via consoleDescription(). */
  description: string;
  control: SettingControl;
  bind: string;
}

export const SETTING_DESCRIPTORS: SettingDescriptor[] = [
  { key: 'spp', panel: 'Display', title: 'Strips per screen', description: 'Fewer strips are wider and easier to hit. Swipe to reach the rest.', control: 'segmented', bind: 'setting:spp' },
  { key: 'theme', panel: 'Display', title: 'Theme', description: 'Console Blue, Graphite, Tungsten or Daylight. Meters and accent colours stay put.', control: 'segmented', bind: 'setting:theme' },
  { key: 'peakHold', panel: 'Display', title: 'Meter peak hold', description: 'Keeps a line at the highest level for 1.5 s.', control: 'toggle', bind: 'setting:peakHold' },
  { key: 'fine', panel: 'Faders', title: 'Fine control', description: 'Faders and knobs move a quarter as far per drag.', control: 'toggle', bind: 'setting:fine' },
  { key: 'sof', panel: 'Faders', title: 'Sends on faders', description: 'Strip faders set sends to the target mix. The master strip shows that mix.', control: 'toggle', bind: 'setting:sof' },
  { key: 'mix', panel: 'Faders', title: 'Send target', description: 'Which mix the faders control when sends are on.', control: 'pads', bind: 'setting:mix' },
  { key: 'muteEnable', panel: 'Safety', title: 'Mute Enable', description: 'When off, mute buttons ignore taps.', control: 'toggle', bind: 'setting:muteEnable' },
  { key: 'locked', panel: 'Safety', title: 'Screen lock', description: 'Blocks faders, knobs, mutes and patching. Unlock from the top bar.', control: 'toggle', bind: 'setting:locked' },
  { key: 'sceneConfirm', panel: 'Safety', title: 'Scene recall', description: 'Confirm needs a press on RECALL. Instant loads on tap.', control: 'segmented', bind: 'setting:sceneConfirm' },
  { key: 'appVersion', panel: 'About', title: 'App version', description: '[VERSION]', control: 'none', bind: 'setting:appVersion' },
  { key: 'console', panel: 'About', title: 'Console', description: '', control: 'none', bind: 'setting:connected' },
];

/** Validate a descriptor list (used by tests and the layout editor). */
export function validateSettingDescriptors(list: SettingDescriptor[]): string[] {
  const errors: string[] = [];
  if (!Array.isArray(list) || list.length === 0) return ['descriptor list must be a non-empty array'];
  const panels: SettingPanelId[] = ['Display', 'Faders', 'Safety', 'About'];
  const controls: SettingControl[] = ['segmented', 'toggle', 'pads', 'none'];
  const seen = new Set<string>();
  for (let i = 0; i < list.length; i++) {
    const d = list[i];
    const at = `descriptors[${i}]`;
    if (!d || typeof d.key !== 'string' || d.key.length === 0) { errors.push(`${at}.key must be a non-empty string`); continue; }
    if (seen.has(d.key)) errors.push(`${at}.key "${d.key}" is duplicated`);
    seen.add(d.key);
    if (!panels.includes(d.panel)) errors.push(`${at}.panel "${String(d.panel)}" must be one of ${panels.join('|')}`);
    if (typeof d.title !== 'string' || d.title.length === 0) errors.push(`${at}.title must be a non-empty string`);
    if (typeof d.description !== 'string') errors.push(`${at}.description must be a string`);
    if (!controls.includes(d.control)) errors.push(`${at}.control "${String(d.control)}" must be one of ${controls.join('|')}`);
    if (typeof d.bind !== 'string' || !d.bind.startsWith('setting:')) errors.push(`${at}.bind "${String(d.bind)}" must be a "setting:*" binding`);
  }
  return errors;
}

/** Console row description (port of the design's About/Console desc). */
export function consoleDescription(connected: string | null): string {
  return connected === 'offline' ? 'Offline' : connected ? 'FOH / Monitor · 48 kHz' : 'Not connected';
}

export interface SettingOptionVm {
  label: string;
  /** String-encoded value the screen maps back to the store action. */
  value: string;
  pressed: boolean;
  bg: string;
  fg: string;
  border: string;
}

export interface SettingRowVm {
  key: string;
  label: string;
  desc: string;
  control: SettingControl;
  options: SettingOptionVm[];
}

export interface SettingsSectionVm {
  title: SettingPanelId;
  rows: SettingRowVm[];
}

function opt(label: string, value: string, on: boolean): SettingOptionVm {
  return {
    label,
    value,
    pressed: on,
    bg: on ? 'var(--u4-seg-on-bg)' : 'var(--u4-seg-opt-bg)',
    fg: on ? 'var(--u4-seg-on-fg)' : 'var(--u4-text-4)',
    border: on ? 'var(--u4-seg-on-bg)' : 'var(--u4-seg-border)',
  };
}

function boolOpts(cur: boolean): SettingOptionVm[] {
  return [opt('Off', 'off', !cur), opt('On', 'on', cur)];
}

function descOf(d: SettingDescriptor, state: UiState): string {
  return d.key === 'console' ? consoleDescription(state.connected) : d.description;
}

export function buildSettingsVm(state: UiState): SettingsSectionVm[] {
  const spp = visibleCount(state.spp);
  const byKey: Record<string, SettingOptionVm[]> = {
    spp: ['8', '12', '16'].map(s => opt(s, s, parseInt(s, 10) === spp)),
    theme: (Object.keys(PRESETS) as ThemePresetId[]).map(k => opt(PRESETS[k].label, k, state.theme === k)),
    peakHold: boolOpts(state.peakHold),
    fine: boolOpts(state.fine),
    sof: boolOpts(state.sof),
    mix: Array.from({ length: 16 }, (_, k) => opt(String(k + 1), String(k), state.mix === k)),
    muteEnable: boolOpts(state.muteEnable),
    locked: boolOpts(state.locked),
    sceneConfirm: [opt('Confirm', 'confirm', state.sceneConfirm), opt('Instant', 'instant', !state.sceneConfirm)],
    appVersion: [],
    console: [],
  };
  const sections: SettingsSectionVm[] = [];
  for (const d of SETTING_DESCRIPTORS) {
    let sec = sections.find(s => s.title === d.panel);
    if (!sec) {
      sec = { title: d.panel, rows: [] };
      sections.push(sec);
    }
    sec.rows.push({ key: d.key, label: d.title, desc: descOf(d, state), control: d.control, options: byKey[d.key] ?? [] });
  }
  return sections;
}

// S910 module manifests (MODULES.md section 2): the Settings/Scenes
// presentational components as registered modules. L1 module form: props in
// via layout `bind`, events out via onEvent (dispatched as actions by the
// layout JSON in ui/layouts/settings.default.json and
// ui/layouts/scenes.default.json, rendered by the generic LayoutRenderer).
//
// Settings rows are data-driven: each `setting-row` binds one row of the
// SETTING_DESCRIPTORS-derived rows (key/label/desc/control/options) and
// emits `change` {key, value} -> ui.setSetting (registered in
// screens/settings/actions.ts). Scenes render via a layout `repeat` over the
// card vms (`scene-card`, select -> ui.pickScene) plus the recall bar module.

import type { Component } from 'svelte';
import type { ModuleManifest, ModuleProps } from '../types.ts';
import SettingRow from '../../screens/settings/SettingRow.svelte';
import SettingsPanel from '../../screens/settings/SettingsPanel.svelte';
import SceneCard from '../../screens/scenes/SceneCard.svelte';
import SceneRecallBar from '../../screens/scenes/SceneRecallBar.svelte';

const asModule = <P>(c: Component<unknown>): Component<ModuleProps<P>> =>
  c as unknown as Component<ModuleProps<P>>;

// eslint-disable-next-line @typescript-eslint/no-explicit-any
type AnyProps = any;

const settingRow: ModuleManifest<AnyProps> = {
  id: 'setting-row',
  title: 'Setting row',
  category: 'control',
  component: asModule<AnyProps>(SettingRow),
  props: {
    rowKey: { type: 'text', default: 'spp' },
    label: { type: 'text', default: 'Strips per screen' },
    desc: { type: 'text', default: '' },
    control: { type: 'enum', options: ['toggle', 'segmented', 'pads', 'none'], default: 'toggle' },
    options: { type: 'list', item: { type: 'object' }, default: [] },
  },
  defaults: { rowKey: 'spp', label: 'Setting', desc: '', control: 'toggle', options: [] },
  size: {},
  // Row values arrive per-instance via layout bind over the descriptor rows;
  // the module only renders what it is given (no store reads of its own).
  binds: [],
  events: [{ name: 'change', args: ['key', 'value'] }],
};

const settingsPanel: ModuleManifest<AnyProps> = {
  id: 'settings-panel',
  title: 'Settings panel',
  category: 'screen',
  component: asModule<AnyProps>(SettingsPanel),
  props: {
    title: { type: 'text', default: 'Display' },
  },
  defaults: { title: 'Display' },
  size: {},
};

const sceneCard: ModuleManifest<AnyProps> = {
  id: 'scene-card',
  title: 'Scene card',
  category: 'control',
  component: asModule<AnyProps>(SceneCard),
  props: {
    index: { type: 'number', min: 0, max: 9, step: 1, default: 0 },
    num: { type: 'text', default: '001' },
    name: { type: 'text', default: 'Scene' },
    tag: { type: 'text', default: '' },
    pressed: { type: 'bool', default: false },
    border: { type: 'color', default: '#222a36' },
    bg: { type: 'color', default: '#0e1520' },
  },
  defaults: { index: 0, num: '001', name: 'Scene', tag: '', pressed: false, border: '#222a36', bg: '#0e1520' },
  size: { minW: 220 },
  binds: [],
  events: [{ name: 'select', args: ['index'] }],
};

const sceneRecallBar: ModuleManifest<AnyProps> = {
  id: 'scene-recall-bar',
  title: 'Scene recall bar',
  category: 'control',
  component: asModule<AnyProps>(SceneRecallBar),
  props: {
    tag: { type: 'text', default: 'Current scene' },
    tagColor: { type: 'color', default: '#7be27b' },
    num: { type: 'text', default: '001' },
    name: { type: 'text', default: 'Scene' },
    barBorder: { type: 'color', default: '#222a36' },
    border: { type: 'color', default: '#2a323f' },
    bg: { type: 'color', default: '#161c26' },
    text: { type: 'color', default: '#5d6878' },
    disabled: { type: 'bool', default: true },
  },
  defaults: {
    tag: 'Current scene', tagColor: '#7be27b', num: '001', name: 'Scene',
    barBorder: '#222a36', border: '#2a323f', bg: '#161c26', text: '#5d6878', disabled: true,
  },
  size: {},
  binds: [],
  events: [{ name: 'cancel' }, { name: 'recall' }],
};

export const manifests: ModuleManifest<AnyProps>[] = [
  settingRow,
  settingsPanel,
  sceneCard,
  sceneRecallBar,
];

export default manifests;

// W5b Channel package: module manifests (one per presentational component).
//
// L1-converted like s78-pages: components are presentational ModuleProps
// components (props in via layout `bind`, events out via `onEvent`); the
// screens bind them to uiStore + ui.do through layout JSON. `binds`/
// `requires` are omitted: these modules read fixture/store data through
// bound props (no catalogue bindings yet — engine binding is wave 3), and
// need no desk caps, so fixture mode (caps []) renders everything. Caps
// honesty lives in props instead: `dyn-knob-panel` filters its data-driven
// knob list by the bound `caps`, `eq-hpf-card` collapses its slope row via
// `showSlope`.
//
// Extension API (MODULES.md §4): picked up by loadManifestModules() via
// import.meta.glob — no other wiring needed.

import type { Component } from 'svelte';
import type { ModuleManifest, ModuleProps } from '../types.ts';
import ChannelTabButton from '../../screens/channel/ChannelTabButton.svelte';
import GroupsCard from '../../screens/channel/GroupsCard.svelte';
import StripNameChip from '../../screens/channel/StripNameChip.svelte';
import StripStepper from '../../screens/channel/StripStepper.svelte';
import StripPanBar from '../../screens/channel/StripPanBar.svelte';
import StripMute from '../../screens/channel/StripMute.svelte';
import StripFader from '../../screens/channel/StripFader.svelte';
import StripReadout from '../../screens/channel/StripReadout.svelte';
import StripSourceLabel from '../../screens/channel/StripSourceLabel.svelte';
import PreampCard from '../../screens/channel/PreampCard.svelte';
import SourceCard from '../../screens/channel/SourceCard.svelte';
import DelayCard from '../../screens/channel/DelayCard.svelte';
import SendRow from '../../screens/channel/SendRow.svelte';

function as<P>(c: unknown): Component<ModuleProps<P>> {
  return c as Component<ModuleProps<P>>;
}

const vmObject = { type: 'object' as const };

export const manifests: ModuleManifest[] = [
  {
    id: 'channel-tab-button',
    title: 'Channel tab button',
    category: 'channel',
    component: as(ChannelTabButton),
    props: { entry: vmObject },
    defaults: {},
    size: { fixedH: 56, flex: true },
    events: [{ name: 'pick', args: ['id'] }],
  },
  {
    id: 'channel-name-chip',
    title: 'Channel name chip',
    category: 'channel',
    component: as(StripNameChip),
    props: {
      name: { type: 'text', default: '' },
      color: { type: 'color', default: 'var(--u4-btn-bg)' },
      text: { type: 'color', default: 'var(--u4-text-2)' },
    },
    defaults: { name: '', color: 'var(--u4-btn-bg)', text: 'var(--u4-text-2)' },
    size: {},
  },
  {
    id: 'channel-stepper',
    title: 'Channel prev/next stepper',
    category: 'channel',
    component: as(StripStepper),
    props: {},
    defaults: {},
    size: {},
    events: [{ name: 'prev' }, { name: 'next' }],
  },
  {
    id: 'channel-pan-bar',
    title: 'Channel pan bar',
    category: 'channel',
    component: as(StripPanBar),
    props: {
      value: { type: 'number', min: -1, max: 1, step: 0.01, default: 0 },
      name: { type: 'text', default: '' },
      fine: { type: 'bool', default: false },
    },
    defaults: { value: 0, name: '', fine: false },
    size: {},
    events: [{ name: 'input', args: ['value'] }],
  },
  {
    id: 'channel-mute',
    title: 'Channel mute button',
    category: 'channel',
    component: as(StripMute),
    props: {
      muted: { type: 'bool', default: false },
      enabled: { type: 'bool', default: true },
      label: { type: 'text', default: 'Mute' },
    },
    defaults: { muted: false, enabled: true, label: 'Mute' },
    size: {},
    events: [{ name: 'press' }],
  },
  {
    id: 'channel-fader',
    title: 'Channel fader + meter',
    category: 'channel',
    component: as(StripFader),
    props: {
      pos: { type: 'number', min: 0, max: 1, step: 0.001, default: 0.75 },
      readout: { type: 'text', default: '' },
      aria: { type: 'text', default: 'Fader' },
      accent: { type: 'enum', options: ['chan', 'sof'], default: 'chan' },
      readColor: { type: 'color', default: 'var(--u4-text-4)' },
      fine: { type: 'bool', default: false },
      meter: vmObject,
      meterFrozen: { type: 'bool', default: false },
    },
    defaults: { pos: 0.75, readout: '', accent: 'chan', fine: false },
    size: { flex: true },
    events: [{ name: 'input', args: ['value'] }],
  },
  {
    id: 'channel-readout',
    title: 'Channel dB readout',
    category: 'channel',
    component: as(StripReadout),
    props: {
      text: { type: 'text', default: '' },
      color: { type: 'color', default: 'var(--u4-text-4)' },
    },
    defaults: { text: '', color: 'var(--u4-text-4)' },
    size: {},
  },
  {
    id: 'channel-source-label',
    title: 'Channel source label',
    category: 'channel',
    component: as(StripSourceLabel),
    props: { src: { type: 'text', default: '' } },
    defaults: { src: '' },
    size: {},
  },
  {
    id: 'input-preamp-card',
    title: 'Input preamp card',
    category: 'channel',
    component: as(PreampCard),
    props: { vm: vmObject, fine: { type: 'bool', default: false } },
    defaults: { fine: false },
    size: { flex: true, minW: 270 },
    events: [{ name: 'knob', args: ['key', 'value'] }, { name: 'phantom' }, { name: 'invert' }],
  },
  {
    id: 'input-source-card',
    title: 'Input source card',
    category: 'channel',
    component: as(SourceCard),
    props: { vm: vmObject, levelText: { type: 'text', default: '' }, meter: vmObject, meterFrozen: { type: 'bool', default: false } },
    defaults: { levelText: '', meterFrozen: false },
    size: { flex: true, minW: 270 },
    events: [{ name: 'patch' }],
  },
  {
    id: 'input-delay-card',
    title: 'Input delay card',
    category: 'channel',
    component: as(DelayCard),
    props: { vm: vmObject, fine: { type: 'bool', default: false } },
    defaults: { fine: false },
    size: { flex: true, minW: 270 },
    events: [{ name: 'knob', args: ['key', 'value'] }, { name: 'toggle' }],
  },
  {
    id: 'send-row',
    title: 'Send row',
    category: 'channel',
    component: as(SendRow),
    props: { row: vmObject, fine: { type: 'bool', default: false } },
    defaults: { fine: false },
    size: { minW: 320, fixedH: 58 },
    events: [{ name: 'input', args: ['value'] }, { name: 'togglePre' }],
  },
  {
    id: 'channel-groups-card',
    title: 'Channel desk-groups card',
    category: 'channel',
    component: as(GroupsCard),
    props: { vm: vmObject },
    defaults: {},
    size: { flex: true, minW: 270 },
    events: [{ name: 'assign', args: ['kind', 'group', 'member'] }, { name: 'master', args: ['kind', 'group', 'muted'] }],
  },
];

export default manifests;

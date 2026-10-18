// W5c module manifests: every mixer strip part as a ModuleProps module
// (MODULES.md §1-2), rendered by the default layouts (mixer.default.json +
// strip.default.json) through LayoutRenderer.
//
// Extension API (MODULES.md §4): picked up by loadManifestModules() via
// import.meta.glob — no other wiring needed. Components are presentational
// (props in via layout `bind`, events out via `onEvent`); the MixerScreen
// binds them to uiStore + ui.do through layout JSON. Data arrives through
// bound props (no catalogue bindings yet — engine binding is wave 3), so
// `binds`/`requires` are omitted like the S78 manifests: per-strip
// conditionals (pan/EQ collapse, empty slots, SOF accents) live in the
// layout `when`/`bind` strings and the modules' own null handling.

import type { Component } from 'svelte';
import type { ModuleManifest, ModuleProps } from '../types.ts';
import StripPan from '../../screens/mixer/StripPan.svelte';
import StripFrame from '../../screens/mixer/StripFrame.svelte';
import StripMute from '../../screens/mixer/StripMute.svelte';
import StripFader from '../../screens/mixer/StripFader.svelte';
import StripName from '../../screens/mixer/StripName.svelte';
import StripNameEditor from '../../screens/mixer/StripNameEditor.svelte';
import StripSolo from '../../screens/mixer/StripSolo.svelte';
import StripEmpty from '../../screens/mixer/StripEmpty.svelte';
import StripWellEmpty from '../../screens/mixer/StripWellEmpty.svelte';
import DbReadout from '../../screens/mixer/DbReadout.svelte';
import EqMini from '../../screens/mixer/EqMini.svelte';
import DynMini from '../../screens/mixer/DynMini.svelte';

function as<P>(c: unknown): Component<ModuleProps<P>> {
  return c as Component<ModuleProps<P>>;
}

const inputEvent = [{ name: 'input', args: ['value'] }];

export const manifests: ModuleManifest[] = [
  {
    id: 'strip-frame',
    title: 'Mixer strip frame',
    category: 'strip',
    component: as(StripFrame),
    props: {
      cell: { type: 'object' },
      index: { type: 'number', default: 0 },
      store: { type: 'object' },
      frozen: { type: 'bool', default: false },
      tree: { type: 'object' },
    },
    defaults: { index: 0, frozen: false },
    size: { flex: true },
  },
  {
    id: 'strip-pan',
    title: 'Strip pan bar',
    category: 'strip',
    component: as(StripPan),
    props: {
      value: { type: 'number', min: -1, max: 1, step: 0.01, default: 0 },
      name: { type: 'text', default: 'Ch 1' },
      variant: { type: 'enum', options: ['strip', 'aside'], default: 'strip' },
      disabled: { type: 'bool', default: false },
      fine: { type: 'bool', default: false },
    },
    defaults: { value: 0, name: 'Ch 1', variant: 'strip', disabled: false, fine: false },
    size: { minW: 60, fixedH: 30 },
    events: inputEvent,
  },
  {
    id: 'strip-eq-mini',
    title: 'Strip EQ mini pane',
    category: 'strip',
    component: as(EqMini),
    props: {
      path: { type: 'text', default: '' },
      stroke: { type: 'color', default: '#d9c24a' },
      ariaLabel: { type: 'text', default: 'Open EQ for Ch 1' },
    },
    defaults: { path: '', stroke: '#d9c24a', ariaLabel: 'Open EQ for Ch 1' },
    size: { flex: true },
    events: [{ name: 'open' }],
  },
  {
    id: 'strip-dyn-mini',
    title: 'Strip dynamics mini pane',
    category: 'strip',
    component: as(DynMini),
    props: {
      path: { type: 'text', default: '' },
      ariaLabel: { type: 'text', default: 'Open dynamics for Ch 1' },
    },
    defaults: { path: '', ariaLabel: 'Open dynamics for Ch 1' },
    size: { flex: true },
    events: [{ name: 'open' }],
  },
  {
    id: 'strip-mute',
    title: 'Strip mute button',
    category: 'strip',
    component: as(StripMute),
    props: {
      muted: { type: 'bool', default: false },
      enabled: { type: 'bool', default: true },
      height: { type: 'number', min: 30, max: 56, default: 40 },
      label: { type: 'text', default: 'Mute Ch 1' },
    },
    defaults: { muted: false, enabled: true, height: 40, label: 'Mute Ch 1' },
    size: { minW: 60, minH: 30, fixedH: 40 },
    events: [{ name: 'press' }],
  },
  {
    id: 'strip-readout',
    title: 'Strip dB readout',
    category: 'display',
    component: as(DbReadout),
    props: {
      text: { type: 'text', default: '-4.3' },
      color: { type: 'color', default: '#c9d1dc' },
    },
    defaults: { text: '-4.3', color: '#c9d1dc' },
    size: { minW: 60, fixedH: 15 },
  },
  {
    id: 'strip-fader',
    title: 'Strip fader',
    category: 'strip',
    component: as(StripFader),
    props: {
      value: { type: 'number', min: 0, max: 1, default: 0.75 },
      dbText: { type: 'text', default: '-4.3' },
      ariaLabel: { type: 'text', default: 'Fader Ch 1 -4.3 dB' },
      variant: { type: 'enum', options: ['strip', 'aside', 'master'], default: 'strip' },
      accent: { type: 'enum', options: ['chan', 'sof', 'master'], default: 'chan' },
      readColor: { type: 'color', default: '#c9d1dc' },
      disabled: { type: 'bool', default: false },
      fine: { type: 'bool', default: false },
      meter: { type: 'object' },
      meterFrozen: { type: 'bool', default: false },
    },
    defaults: { value: 0.75, variant: 'strip', accent: 'chan', disabled: false, fine: false },
    size: { minW: 60, minH: 180, flex: true },
    events: inputEvent,
  },
  {
    id: 'strip-name',
    title: 'Strip name bar',
    category: 'strip',
    component: as(StripName),
    props: {
      name: { type: 'text', default: 'Ch 1' },
      sub: { type: 'text', default: 'Lcl 01' },
      color: { type: 'color', default: '#5cdb5c' },
      text: { type: 'color', default: '#10240f' },
      variant: { type: 'enum', options: ['strip', 'plate'], default: 'strip' },
      selectAria: { type: 'text', default: 'Select Ch 1, source Lcl 01' },
    },
    defaults: { variant: 'strip' },
    size: { minW: 60, fixedH: 52 },
    events: [{ name: 'select' }],
  },
  {
    id: 'strip-name-editor',
    title: 'Strip name editor',
    category: 'strip',
    component: as(StripNameEditor),
    props: {
      name: { type: 'text', default: 'Ch 1' },
      selectedColor: { type: 'color', default: '#5cdb5c' },
    },
    defaults: {},
    size: { minW: 60 },
    events: [{ name: 'rename' }, { name: 'recolor' }, { name: 'close' }],
  },
  {
    id: 'strip-solo',
    title: 'Strip solo button',
    category: 'strip',
    component: as(StripSolo),
    props: {
      solo: { type: 'bool', default: false },
      enabled: { type: 'bool', default: true },
      height: { type: 'number', min: 30, max: 56, default: 40 },
      label: { type: 'text', default: 'Solo Ch 1' },
    },
    defaults: { solo: false, enabled: true, height: 40, label: 'Solo Ch 1' },
    size: { minW: 60, minH: 30, fixedH: 40 },
    events: [{ name: 'press' }],
  },
  {
    id: 'strip-empty',
    title: 'Empty bank slot',
    category: 'strip',
    component: as(StripEmpty),
    props: {},
    defaults: {},
    size: { flex: true },
  },
  {
    id: 'strip-well-empty',
    title: 'Empty proc well',
    category: 'strip',
    component: as(StripWellEmpty),
    props: {},
    defaults: {},
    size: { flex: true },
  },
];

export default manifests;

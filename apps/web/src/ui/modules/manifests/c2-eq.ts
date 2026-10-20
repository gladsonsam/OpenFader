// W5b EQ package: module manifests (one per presentational component).
//
// L1-converted like s78-pages: components are presentational ModuleProps
// components (props in via layout `bind`, events out via `onEvent`); the
// screens bind them to uiStore + ui.do through layout JSON. `binds`/
// `requires` are omitted: these modules read fixture/store data through
// bound props (no catalogue bindings yet — engine binding is wave 3), and
// need no desk caps, so fixture mode (caps []) renders everything. Caps
// honesty lives in props: `eq-hpf-card` collapses its slope row via
// `showSlope`.
//
// Extension API (MODULES.md §4): picked up by loadManifestModules() via
// import.meta.glob — no other wiring needed.

import type { Component } from 'svelte';
import type { ModuleManifest, ModuleProps } from '../types.ts';
import EqGraph from '../../screens/channel/eq/EqGraph.svelte';
import EqHead from '../../screens/channel/eq/EqHead.svelte';
import BandCard from '../../screens/channel/eq/BandCard.svelte';
import HpfCard from '../../screens/channel/eq/HpfCard.svelte';
import EqSummary from '../../screens/channel/eq/EqSummary.svelte';

function as<P>(c: unknown): Component<ModuleProps<P>> {
  return c as Component<ModuleProps<P>>;
}

const vmObject = { type: 'object' as const };

export const manifests: ModuleManifest[] = [
  {
    id: 'eq-graph',
    title: 'EQ response graph',
    category: 'channel',
    component: as(EqGraph),
    props: {
      graph: vmObject,
      locked: { type: 'bool', default: false },
      fine: { type: 'bool', default: false },
      selBand: { type: 'number', min: 0, max: 6, step: 1, default: 1 },
      selQ: { type: 'number', min: 0, max: 16, step: 0.01, default: 0 },
    },
    defaults: { locked: false, fine: false, selBand: 1, selQ: 0 },
    size: { minH: 220, flex: true },
    events: [
      { name: 'select', args: ['band'] },
      { name: 'node', args: ['band', 'freq', 'gain'] },
      { name: 'q', args: ['band', 'q'] },
    ],
  },
  {
    id: 'eq-head',
    title: 'EQ head buttons',
    category: 'channel',
    component: as(EqHead),
    props: {
      label: { type: 'text', default: '' },
      bg: { type: 'color', default: 'var(--u4-btn-bg)' },
      fg: { type: 'color', default: 'var(--u4-text-1)' },
      pressed: { type: 'text', default: 'false' },
      ariaLabel: { type: 'text', default: 'EQ in' },
      locked: { type: 'bool', default: false },
    },
    defaults: { label: '', pressed: 'false' },
    size: {},
    events: [{ name: 'toggle' }, { name: 'flat' }, { name: 'reset' }],
  },
  {
    id: 'eq-band-card',
    title: 'EQ band card',
    category: 'channel',
    component: as(BandCard),
    props: {
      card: vmObject,
      showType: { type: 'bool', default: true },
      fine: { type: 'bool', default: false },
      locked: { type: 'bool', default: false },
    },
    defaults: { showType: true, fine: false, locked: false },
    size: { minW: 180, flex: true },
    events: [
      { name: 'select', args: ['band'] },
      { name: 'toggle', args: ['band'] },
      { name: 'type', args: ['band', 'type'] },
      { name: 'param', args: ['band', 'param', 'value'] },
    ],
  },
  {
    id: 'eq-hpf-card',
    title: 'HPF card',
    category: 'channel',
    component: as(HpfCard),
    props: {
      hpf: vmObject,
      showSlope: { type: 'bool', default: true },
      fine: { type: 'bool', default: false },
      locked: { type: 'bool', default: false },
    },
    defaults: { showSlope: true, fine: false, locked: false },
    size: { minW: 180, flex: true },
    events: [
      { name: 'select', args: [] },
      { name: 'toggle', args: [] },
      { name: 'slope', args: ['slope'] },
      { name: 'freq', args: ['value'] },
    ],
  },
  {
    id: 'eq-summary',
    title: 'EQ selected-band summary',
    category: 'channel',
    component: as(EqSummary),
    props: {
      text: { type: 'text', default: '' },
    },
    defaults: { text: '' },
    size: { minW: 160, flex: true },
  },
];

export default manifests;

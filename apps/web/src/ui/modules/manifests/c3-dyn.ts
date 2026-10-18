// W5b Gate+Compressor package: module manifests (one per presentational
// component).
//
// L1-converted like s78-pages: components are presentational ModuleProps
// components (props in via layout `bind`, events out via `onEvent`); the
// screens bind them to uiStore + ui.do through layout JSON. `binds`/
// `requires` are omitted: these modules read fixture/store data through
// bound props (no catalogue bindings yet — engine binding is wave 3), and
// need no desk caps, so fixture mode (caps []) renders everything. Caps
// honesty lives in props: `dyn-knob-panel` removes knobs whose bind is
// absent from the bound `caps` (null = all present).
//
// Extension API (MODULES.md §4): picked up by loadManifestModules() via
// import.meta.glob — no other wiring needed.

import type { Component } from 'svelte';
import type { ModuleManifest, ModuleProps } from '../types.ts';
import DynSummary from '../../screens/channel/dyn/DynSummary.svelte';
import TransferGraph from '../../screens/channel/dyn/TransferGraph.svelte';
import DynKnobPanel from '../../screens/channel/dyn/DynKnobPanel.svelte';

function as<P>(c: unknown): Component<ModuleProps<P>> {
  return c as Component<ModuleProps<P>>;
}

const vmObject = { type: 'object' as const };

const kindProp = {
  type: 'enum' as const,
  options: ['gate', 'comp'],
  default: 'comp',
};

const colorProp = { type: 'color' as const, default: '#ff9f43' };

// Knob list as data (prop keys into DYN_KNOBS): users add/remove knobs by
// editing this list; the panel resolves label/spec/bind per entry.
const knobsProp = {
  type: 'list' as const,
  item: { type: 'object' as const },
  default: [] as unknown[],
};

export const manifests: ModuleManifest[] = [
  {
    id: 'dyn-transfer-graph',
    title: 'Dynamics transfer graph',
    category: 'channel',
    component: as(TransferGraph),
    props: {
      kind: kindProp,
      color: colorProp,
      aria: { type: 'text', default: '' },
      path: { type: 'text', default: '' },
      fill: { type: 'text', default: '' },
      fillColor: { type: 'text', default: '' },
      thrX: { type: 'text', default: '' },
      thrLeft: { type: 'text', default: '' },
      thrText: { type: 'text', default: '' },
      grid: vmObject,
      axis: vmObject,
      params: vmObject,
      frozen: { type: 'bool', default: true },
      staticDotL: { type: 'text', default: '0' },
      staticDotT: { type: 'text', default: '100' },
      getLevel: vmObject,
    },
    defaults: { kind: 'comp', color: '#ff9f43', frozen: true },
    size: { minW: 400, minH: 300, flex: true },
    events: [
      { name: 'thr', args: ['value'] },
      { name: 'ratio', args: ['value'] },
      { name: 'makeup', args: ['value'] },
    ],
  },
  {
    id: 'dyn-knob-panel',
    title: 'Dynamics knob panel',
    category: 'channel',
    component: as(DynKnobPanel),
    props: {
      kind: kindProp,
      color: colorProp,
      size: { type: 'number', min: 40, max: 160, step: 4, unit: 'px', default: 80 },
      knobs: knobsProp,
      gate: vmObject,
      comp: vmObject,
      fine: { type: 'bool', default: false },
      locked: { type: 'bool', default: false },
      caps: vmObject,
    },
    defaults: { kind: 'comp', color: '#ff9f43', fine: false, locked: false },
    size: { minW: 300, flex: true },
    events: [{ name: 'knob', args: ['prop', 'value'] }],
  },
  {
    id: 'dyn-summary',
    title: 'Dynamics summary',
    category: 'channel',
    component: as(DynSummary),
    props: {
      kind: kindProp,
      label: { type: 'text', default: '' },
      bg: { type: 'color', default: 'var(--u4-btn-bg)' },
      fg: { type: 'color', default: 'var(--u4-text-4)' },
      pressed: { type: 'text', default: 'false' },
      summary: { type: 'text', default: '' },
    },
    defaults: { kind: 'comp' },
    size: { minW: 270, fixedH: 40 },
    events: [{ name: 'toggle' }],
  },
];

export default manifests;

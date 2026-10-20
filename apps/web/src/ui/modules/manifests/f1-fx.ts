// F1 FX package: module manifests (one per presentational component).
//
// L1 module form: props in via layout `bind`, events out via onEvent
// (dispatched as actions by ui/layouts/fx.default.json, rendered by the
// generic LayoutRenderer). FxScreen.svelte only provides scope + profile +
// renderer; the JSON owns the composition: a grid with a `repeat` over the
// slot vms (fx-slot-card, select -> ui.setFxSel) and one `fx-detail-panel`
// reading the selected slot (toggle/tap/pickType/knob/reset -> ui.*).
// The knob row stays descriptor-driven: the screen binds the
// FX_PARAM_DEFS/caps table list and fx-param-knobs renders it (knobs whose
// param cap is absent are removed from the bound list, never faked).
//
// Extension API (MODULES.md §4): picked up by loadManifestModules() via
// import.meta.glob — no other wiring needed.

import type { Component } from 'svelte';
import type { Binding, ModuleManifest, ModuleProps } from '../types.ts';
import FxSlotCard from '../../screens/fx/FxSlotCard.svelte';
import FxDetailPanel from '../../screens/fx/FxDetailPanel.svelte';
import FxTypeSelector from '../../screens/fx/FxTypeSelector.svelte';
import FxParamKnobs from '../../screens/fx/FxParamKnobs.svelte';

function as<P>(c: unknown): Component<ModuleProps<P>> {
  return c as Component<ModuleProps<P>>;
}

/** Catalogue binds use the fx:* namespace (wave 3 resolves them). */
function fxBinds(...ids: string[]): ModuleManifest['binds'] {
  return ids as unknown as Binding[];
}

export const manifests: ModuleManifest[] = [
  {
    id: 'fx-slot-card',
    title: 'FX slot card',
    category: 'screen',
    component: as(FxSlotCard),
    props: {
      index: { type: 'number', min: 0, max: 15, step: 1, default: 0 },
      name: { type: 'text', default: 'FX 1' },
      type: { type: 'text', default: 'Hall' },
      code: { type: 'text', default: '' },
      art: { type: 'object', default: {} },
      state: { type: 'text', default: 'On' },
      stateColor: { type: 'color', default: '#7be27b' },
      ret: { type: 'text', default: 'Ret 1/2' },
      pressed: { type: 'bool', default: false },
      border: { type: 'color', default: 'var(--u4-line-faint)' },
      bg: { type: 'color', default: 'var(--u4-panel-bg)' },
      meterPct: { type: 'text', default: '0' },
    },
    defaults: { index: 0, name: 'FX 1', type: 'Hall' },
    size: { minW: 140, fixedH: 96 },
    binds: fxBinds('fx:slot', 'fx:bypass', 'meter:post'),
    requires: ['fx'],
    fallback: 'remove',
    events: [{ name: 'select', args: ['index'] }],
  },
  {
    id: 'fx-detail-panel',
    title: 'FX detail panel',
    category: 'screen',
    component: as(FxDetailPanel),
    props: {
      title: { type: 'text', default: 'FX 1 · Hall' },
      toggle: { type: 'object', default: {} },
      isDelay: { type: 'bool', default: false },
      tapLabel: { type: 'text', default: 'Tap tempo' },
      types: { type: 'list', item: { type: 'object' }, default: [] },
      slot: { type: 'object', default: {} },
      knobs: { type: 'list', item: { type: 'object' }, default: [] },
      locked: { type: 'bool', default: false },
      fine: { type: 'bool', default: false },
    },
    defaults: { title: 'FX 1 · Hall', locked: false, fine: false },
    size: { flex: true, minW: 700, minH: 300 },
    binds: fxBinds('fx:slot', 'fx:type', 'fx:bypass', 'setting:fine'),
    requires: ['fx'],
    fallback: 'remove',
    events: [
      { name: 'toggle' },
      { name: 'tap' },
      { name: 'pickType', args: ['type'] },
      { name: 'knob', args: ['param', 'value'] },
      { name: 'reset', args: ['param'] },
    ],
  },
  {
    id: 'fx-type-selector',
    title: 'FX type selector',
    category: 'screen',
    component: as(FxTypeSelector),
    props: {
      types: { type: 'list', item: { type: 'object' }, default: [] },
    },
    defaults: { types: [] },
    size: { minW: 600 },
    binds: fxBinds('fx:type'),
    requires: ['fx'],
    fallback: 'remove',
    events: [{ name: 'pickType', args: ['type'] }],
  },
  {
    id: 'fx-param-knobs',
    title: 'FX param knobs',
    category: 'screen',
    component: as(FxParamKnobs),
    props: {
      slot: { type: 'object', default: {} },
      knobs: {
        type: 'list',
        item: { type: 'object' },
        default: [],
      },
      fine: { type: 'bool', default: false },
      locked: { type: 'bool', default: false },
    },
    defaults: { slot: {}, knobs: [], fine: false, locked: false },
    size: { flex: true, minW: 600, minH: 200 },
    binds: fxBinds('fx:param:decay', 'fx:param:predelay', 'fx:param:size', 'fx:param:hidamp', 'fx:param:mix', 'setting:fine'),
    requires: ['fx'],
    fallback: 'remove',
    events: [
      { name: 'knob', args: ['param', 'value'] },
      { name: 'reset', args: ['param'] },
    ],
  },
];

export default manifests;

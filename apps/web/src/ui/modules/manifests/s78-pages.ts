// S78 module manifests, L1-converted: routing + connection leaf modules.
// (The generic `btn` leaf used to live here; it moved to the layout
// primitives file so the mixer paint path needs one less package.)
//
// Extension API (MODULES.md §4): picked up by loadManifestModules() via
// import.meta.glob — no other wiring needed. Components are presentational
// (props in via layout `bind`, `press`/`connect` events out via `onEvent`);
// the screens bind them to uiStore + ui.do through layout JSON. `binds`/
// `requires` are declared honestly: these modules read data through bound
// props (no catalogue bindings yet — engine binding is wave 3), and need no
// desk caps, so both are omitted.

import type { Component } from 'svelte';
import type { ModuleManifest, ModuleProps } from '../types.ts';
import RouteTile from '../../screens/routing/RouteTile.svelte';
import RouteMatrix from '../../screens/routing/RouteMatrix.svelte';
import SrcTile from '../../screens/routing/SrcTile.svelte';
import StatusDot from '../../screens/connection/StatusDot.svelte';
import DeviceRow from '../../screens/connection/DeviceRow.svelte';
import ConnectionFlow from '../../screens/connection/ConnectionFlow.svelte';
import ManualDesk from '../../screens/connection/ManualDesk.svelte';

function as<P>(c: unknown): Component<ModuleProps<P>> {
  return c as Component<ModuleProps<P>>;
}

const pressEvent = [{ name: 'press' }];

export const manifests: ModuleManifest[] = [
  {
    id: 'connection-flow', title: 'Mixer connection flow', category: 'screen',
    component: as(ConnectionFlow), props: {}, defaults: {}, size: { flex: true },
  },
  {
    id: 'route-tile',
    title: 'Routing channel tile',
    category: 'screen',
    component: as(RouteTile),
    props: {
      name: { type: 'text', default: 'Ch 1' },
      src: { type: 'text', default: '—' },
      srcColor: { type: 'color', default: '#5fe3f0' },
      pressed: { type: 'text', default: 'false' },
      border: { type: 'text', default: 'var(--u4-line-faint)' },
      bg: { type: 'color', default: 'var(--u4-well-bg)' },
    },
    defaults: { name: 'Ch 1', src: '—' },
    size: { flex: true, minW: 76 },
    events: pressEvent,
  },
  {
    id: 'src-tile',
    title: 'Routing source tile',
    category: 'screen',
    component: as(SrcTile),
    props: {
      label: { type: 'text', default: 'Lcl 01' },
      used: { type: 'text', default: '' },
      pressed: { type: 'text', default: 'false' },
      bg: { type: 'color', default: 'var(--u4-well-bg)' },
      fg: { type: 'color', default: 'var(--u4-text-7)' },
      border: { type: 'text', default: 'var(--u4-line-faint)' },
    },
    defaults: { label: 'Lcl 01' },
    size: { minW: 76 },
    events: pressEvent,
  },
  {
    id: 'route-matrix',
    title: 'Routing patchbay matrix',
    category: 'screen',
    component: as(RouteMatrix),
    props: {
      rows: { type: 'object', default: [] },
      cols: { type: 'object', default: [] },
      used: { type: 'object', default: {} },
      cell: { type: 'number', min: 32, max: 80, step: 2, unit: 'px', default: 68 },
      headW: { type: 'number', min: 80, max: 260, step: 2, unit: 'px', default: 220 },
    },
    defaults: {},
    size: {},
    events: pressEvent,
  },
  {
    id: 'status-dot',
    title: 'Connection status dot',
    category: 'screen',
    component: as(StatusDot),
    props: {
      color: { type: 'color', default: '#5ce65c' },
      halo: { type: 'text', default: 'rgba(95,224,96,.18)' },
    },
    defaults: { color: '#5ce65c', halo: 'rgba(95,224,96,.18)' },
    size: {},
  },
  {
    id: 'device-row',
    title: 'Connection device row',
    category: 'screen',
    component: as(DeviceRow),
    props: {
      device: { type: 'object' },
    },
    defaults: {},
    size: { flex: true },
    events: [{ name: 'connect', args: ['key'] }],
  },
  {
    id: 'manual-desk',
    title: 'Manual desk add (add-by-IP)',
    category: 'screen',
    component: as(ManualDesk),
    props: {},
    defaults: {},
    size: { flex: true },
    events: [{ name: 'probe-desk', args: ['ip'] }],
  },
];

export default manifests;

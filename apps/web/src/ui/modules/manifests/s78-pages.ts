// S78 module manifests, L1-converted: routing + connection leaf modules.
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
import Btn from '../generic/Btn.svelte';
import RouteTile from '../../screens/routing/RouteTile.svelte';
import SrcTile from '../../screens/routing/SrcTile.svelte';
import StatusDot from '../../screens/connection/StatusDot.svelte';
import DeviceRow from '../../screens/connection/DeviceRow.svelte';
import RelayCard from '../../screens/connection/RelayCard.svelte';

function as<P>(c: unknown): Component<ModuleProps<P>> {
  return c as Component<ModuleProps<P>>;
}

const pressEvent = [{ name: 'press' }];

export const manifests: ModuleManifest[] = [
  {
    id: 'btn',
    title: 'Exact button',
    category: 'control',
    component: as(Btn),
    props: {
      label: { type: 'text', default: 'Button' },
      bg: { type: 'color', default: '#2b3038' },      fg: { type: 'color', default: '#e8edf5' },
      border: { type: 'text', default: '1px solid #3a414c' },
      height: { type: 'number', min: 24, max: 64, step: 2, unit: 'px', default: 36 },
      padding: { type: 'text', default: '0 12px' },
      minWidth: { type: 'number', min: 0, max: 300, step: 1, unit: 'px' },
      fontSize: { type: 'number', min: 10, max: 20, step: 1, unit: 'px', default: 13 },
      pressed: { type: 'text' },
      ariaLabel: { type: 'text' },
      bare: { type: 'bool', default: false },
    },
    defaults: { label: 'Button' },
    size: {},
    events: pressEvent,
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
      border: { type: 'text', default: '#222a36' },
      bg: { type: 'color', default: '#0a0f17' },
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
      bg: { type: 'color', default: '#0a0f17' },
      fg: { type: 'color', default: '#8792a4' },
      border: { type: 'text', default: '#222a36' },
    },
    defaults: { label: 'Lcl 01' },
    size: { minW: 76 },
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
    id: 'relay-card',
    title: 'Relay connection card',
    category: 'screen',
    component: as(RelayCard),
    props: {
      host: { type: 'text', default: '127.0.0.1:8080' },
      phase: { type: 'text', default: 'idle' },
      message: { type: 'text', default: 'Not connected' },
      latencyMs: { type: 'number' },
      strips: { type: 'number', default: 0 },
    },
    defaults: { host: '127.0.0.1:8080', phase: 'idle', message: 'Not connected' },
    size: { flex: true },
    events: [{ name: 'connect', args: ['host'] }, { name: 'disconnect' }],
  },
];

export default manifests;

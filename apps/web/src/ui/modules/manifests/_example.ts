// L0 manifests: layout + container modules (EXAMPLE file — copy this shape).
//
// How to ship a new module (extension API, MODULES.md §4):
//   1. Write a presentational Svelte component taking `{ props, ctx, nodeId }`
//      (+ optional `children` snippet for containers). No backend imports.
//   2. Drop a file in `ui/modules/manifests/<yourpackage>.ts` exporting one
//      manifest, an array, or named `manifest`/`manifests` (all are picked up).
//   3. `loadManifestModules()` (registry.ts) registers them via import.meta.glob.
//   4. Add the module to your screen's DEFAULT LAYOUT JSON in ui/layouts/.
//
// Sizes in the manifests below accept px, fr, %, or plain numbers (= px).

import type { Component } from 'svelte';
import type { ModuleManifest, ModuleProps } from '../types.ts';
import Row from '../layout/Row.svelte';
import Col from '../layout/Col.svelte';
import Grid from '../layout/Grid.svelte';
import Stack from '../layout/Stack.svelte';
import Scroll from '../layout/Scroll.svelte';
import Tabs from '../layout/Tabs.svelte';
import Repeat from '../layout/Repeat.svelte';
import Text from '../layout/Text.svelte';
import Spacer from '../layout/Spacer.svelte';
import Group from '../layout/Group.svelte';
import Missing from '../layout/Missing.svelte';
import Btn from '../generic/Btn.svelte';

function as<P>(c: unknown): Component<ModuleProps<P>> {
  return c as Component<ModuleProps<P>>;
}

const sizeProp = { type: 'text' as const, default: undefined };

export const manifests: ModuleManifest[] = [
  {
    id: 'row',
    title: 'Row',
    category: 'layout',
    component: as(Row),
    props: {
      gap: { type: 'number', min: 0, max: 64, step: 1, unit: 'px', default: 0 },
      align: { type: 'text', default: 'stretch' },
      justify: { type: 'text', default: 'flex-start' },
      wrap: { type: 'bool', default: false },
      width: sizeProp,
      height: sizeProp,
    },
    defaults: { gap: 0 },
    size: { flex: true },
  },
  {
    id: 'col',
    title: 'Column',
    category: 'layout',
    component: as(Col),
    props: {
      gap: { type: 'number', min: 0, max: 64, step: 1, unit: 'px', default: 0 },
      align: { type: 'text', default: 'stretch' },
      justify: { type: 'text', default: 'flex-start' },
      width: sizeProp,
      height: sizeProp,
    },
    defaults: { gap: 0 },
    size: { flex: true },
  },
  {
    id: 'grid',
    title: 'Grid',
    category: 'layout',
    component: as(Grid),
    props: {
      columns: { type: 'text', default: '1fr 1fr' },
      rows: { type: 'text', default: undefined },
      gap: { type: 'number', min: 0, max: 64, step: 1, unit: 'px', default: 0 },
      width: sizeProp,
      height: sizeProp,
    },
    defaults: { columns: '1fr 1fr' },
    size: { flex: true },
  },
  {
    id: 'stack',
    title: 'Stack',
    category: 'layout',
    component: as(Stack),
    props: { width: sizeProp, height: sizeProp },
    defaults: {},
    size: {},
  },
  {
    id: 'scroll',
    title: 'Scroll',
    category: 'layout',
    component: as(Scroll),
    props: {
      direction: { type: 'enum', options: ['row', 'col'], default: 'col' },
      gap: { type: 'number', min: 0, max: 64, step: 1, unit: 'px', default: 0 },
      width: sizeProp,
      height: sizeProp,
    },
    defaults: { direction: 'col' },
    size: { flex: true },
  },
  {
    id: 'tabs',
    title: 'Tabs',
    category: 'layout',
    component: as(Tabs),
    props: {
      tabs: { type: 'list', item: { type: 'text' }, default: [] },
      active: { type: 'number', min: 0, max: 32, step: 1, default: 0 },
    },
    defaults: { tabs: [] },
    size: { flex: true },
  },
  {
    id: 'repeat',
    title: 'Repeat',
    category: 'layout',
    component: as(Repeat),
    props: {
      over: { type: 'ref', kind: 'bank', default: 'strips' },
    },
    defaults: { over: 'strips' },
    size: {},
  },
  {
    id: 'text',
    title: 'Text',
    category: 'display',
    component: as(Text),
    props: {
      value: { type: 'text', default: '' },
      size: { type: 'number', min: 8, max: 64, step: 1, unit: 'px', default: 14 },
      color: { type: 'color', default: 'var(--u4-text-1)' },
      align: { type: 'enum', options: ['left', 'center', 'right'], default: 'left' },
      weight: { type: 'number', min: 100, max: 900, step: 100, default: 400 },
    },
    defaults: { value: '' },
    size: {},
  },
  {
    id: 'spacer',
    title: 'Spacer',
    category: 'layout',
    component: as(Spacer),
    props: {
      w: sizeProp,
      h: sizeProp,
      grow: { type: 'bool', default: false },
    },
    defaults: {},
    size: { flex: true },
  },
  {
    id: 'group',
    title: 'Group',
    category: 'layout',
    component: as(Group),
    props: { width: sizeProp, height: sizeProp },
    defaults: {},
    size: {},
  },
  {
    id: 'missing-module',
    title: 'Missing module placeholder',
    category: 'display',
    component: as(Missing),
    props: { missing: { type: 'text', default: 'unknown' } },
    defaults: {},
    size: {},
    fallback: 'collapse',
  },
  {
    id: 'btn',
    title: 'Exact button',
    category: 'control',
    component: as(Btn),
    props: {
      label: { type: 'text', default: 'Button' },
      icon: { type: 'text' },
      iconSize: { type: 'number', min: 12, max: 32, step: 1, unit: 'px', default: 16 },
      bg: { type: 'color', default: 'var(--u4-btn-bg)' },      fg: { type: 'color', default: 'var(--u4-text-1)' },
      border: { type: 'text', default: '1px solid var(--u4-line-ctl)' },
      height: { type: 'number', min: 24, max: 64, step: 2, unit: 'px', default: 36 },
      flex: { type: 'text' },
      padding: { type: 'text', default: '0 12px' },
      minWidth: { type: 'number', min: 0, max: 300, step: 1, unit: 'px' },
      fontSize: { type: 'number', min: 10, max: 20, step: 1, unit: 'px', default: 13 },
      pressed: { type: 'text' },
      ariaLabel: { type: 'text' },
      disabled: { type: 'bool', default: false },
      bare: { type: 'bool', default: false },
    },
    defaults: { label: 'Button' },
    size: {},
    events: [{ name: 'press' }],
  },
];

export default manifests;

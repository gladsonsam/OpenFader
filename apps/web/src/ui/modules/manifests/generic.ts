// L0 manifests: generic ui-primitive modules used by the L0 demo layout
// (Button / Panel / Segmented from ui/primitives, wrapped as ModuleProps).

import type { Component } from 'svelte';
import type { ModuleManifest, ModuleProps } from '../types.ts';
import ButtonWrap from '../generic/ButtonWrap.svelte';
import PanelWrap from '../generic/PanelWrap.svelte';
import SegWrap from '../generic/SegWrap.svelte';

function as<P>(c: unknown): Component<ModuleProps<P>> {
  return c as Component<ModuleProps<P>>;
}

export const manifests: ModuleManifest[] = [
  {
    id: 'button',
    title: 'Button',
    category: 'control',
    component: as(ButtonWrap),
    props: {
      label: { type: 'text', default: 'Button' },
      variant: { type: 'enum', options: ['default', 'primary', 'danger', 'ghost'], default: 'default' },
      height: { type: 'number', min: 24, max: 64, step: 2, unit: 'px', default: 40 },
    },
    defaults: { label: 'Button' },
    size: {},
  },
  {
    id: 'panel',
    title: 'Panel',
    category: 'screen',
    component: as(PanelWrap),
    props: { title: { type: 'text', default: 'Panel' } },
    defaults: { title: 'Panel' },
    size: { flex: true },
  },
  {
    id: 'segmented',
    title: 'Segmented',
    category: 'control',
    component: as(SegWrap),
    props: {
      groupLabel: { type: 'text', default: 'Options' },
      options: { type: 'list', item: { type: 'text' }, default: [] },
      selected: { type: 'text', default: '' },
    },
    defaults: { groupLabel: 'Options', options: [] },
    size: {},
  },
];

export default manifests;

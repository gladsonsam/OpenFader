// P1 Signal primitives: module manifests (one per presentational component).
// Presentational only: props in, callbacks out; live values arrive via
// ModuleContext bindings (resolved by the renderer from fixtures now, the
// engine in wave 3). Honest UI: every module declares requires/fallback.

import type { Component } from 'svelte';
import type { ModuleManifest, ModuleProps } from '../types.ts';
import Fader from '../../primitives/signal/Fader.svelte';
import Meter from '../../primitives/signal/Meter.svelte';
import Knob from '../../primitives/signal/Knob.svelte';
import MuteButton from '../../primitives/signal/MuteButton.svelte';
import PanBar from '../../primitives/signal/PanBar.svelte';
import NameBar from '../../primitives/signal/NameBar.svelte';
import ScaleLabels from '../../primitives/signal/ScaleLabels.svelte';

// The signal components take plain props (value in, oninput out); the layout
// renderer adapts ModuleProps to them. Casts are adapter intent, not lies:
// each manifest's binds list the exact data the adapter must supply.
const asModule = <P>(c: Component<unknown>): Component<ModuleProps<P>> =>
  c as unknown as Component<ModuleProps<P>>;

// eslint-disable-next-line @typescript-eslint/no-explicit-any
type AnyProps = any;

export const faderManifest: ModuleManifest<AnyProps> = {
  id: 'fader',
  title: 'Fader',
  category: 'strip',
  component: asModule<AnyProps>(Fader),
  props: {
    value: { type: 'number', min: 0, max: 1, default: 0.75 },
    variant: { type: 'enum', options: ['strip', 'aside', 'master'], default: 'strip' },
    accent: { type: 'enum', options: ['chan', 'sof', 'master'], default: 'chan' },
    disabled: { type: 'bool', default: false },
    fine: { type: 'bool', default: false },
  },
  defaults: { value: 0.75, variant: 'strip', accent: 'chan', disabled: false, fine: false },
  size: { minW: 60, minH: 180, flex: true },
  binds: ['param:fader', 'meter:post', 'setting:fine'],
  requires: ['param:fader'],
  fallback: 'disabled',
};

export const meterManifest: ModuleManifest<AnyProps> = {
  id: 'meter',
  title: 'Meter',
  category: 'display',
  component: asModule<AnyProps>(Meter),
  props: {
    variant: { type: 'enum', options: ['strip', 'aside', 'master', 'hbar'], default: 'strip' },
    showPeak: { type: 'bool', default: true },
  },
  defaults: { variant: 'strip', showPeak: true },
  size: { minW: 6, minH: 40 },
  binds: ['meter:post', 'setting:peakHold'],
  requires: ['meter:post'],
  fallback: 'collapse',
};

export const knobManifest: ModuleManifest<AnyProps> = {
  id: 'knob',
  title: 'Knob',
  category: 'control',
  component: asModule<AnyProps>(Knob),
  props: {
    specKey: { type: 'text', default: 'gain' },
    label: { type: 'text', default: 'Gain' },
    color: { type: 'color', default: '#ff9f43' },
    size: { type: 'number', min: 40, max: 120, default: 58 },
    disabled: { type: 'bool', default: false },
    fine: { type: 'bool', default: false },
  },
  defaults: { specKey: 'gain', label: 'Gain', color: '#ff9f43', size: 58, disabled: false, fine: false },
  size: { fixedW: 58, fixedH: 96 },
  binds: ['param:value', 'setting:fine'],
  requires: ['param:value'],
  fallback: 'disabled',
};

export const muteButtonManifest: ModuleManifest<AnyProps> = {
  id: 'mute-button',
  title: 'Mute button',
  category: 'strip',
  component: asModule<AnyProps>(MuteButton),
  props: {
    muted: { type: 'bool', default: false },
    enabled: { type: 'bool', default: true },
    height: { type: 'number', min: 30, max: 56, default: 40 },
  },
  defaults: { muted: false, enabled: true, height: 40 },
  size: { minW: 60, minH: 30, fixedH: 40 },
  binds: ['param:mute', 'setting:muteEnable'],
  fallback: 'disabled',
};

export const panBarManifest: ModuleManifest<AnyProps> = {
  id: 'pan-bar',
  title: 'Pan bar',
  category: 'strip',
  component: asModule<AnyProps>(PanBar),
  props: {
    variant: { type: 'enum', options: ['strip', 'aside'], default: 'strip' },
    disabled: { type: 'bool', default: false },
    fine: { type: 'bool', default: false },
  },
  defaults: { variant: 'strip', disabled: false, fine: false },
  size: { minW: 60, fixedH: 30 },
  binds: ['param:pan', 'setting:fine'],
  requires: ['param:pan'],
  fallback: 'collapse',
};

export const nameBarManifest: ModuleManifest<AnyProps> = {
  id: 'name-bar',
  title: 'Name bar',
  category: 'strip',
  component: asModule<AnyProps>(NameBar),
  props: {
    variant: { type: 'enum', options: ['strip', 'plate'], default: 'strip' },
  },
  defaults: { variant: 'strip' },
  size: { minW: 60, fixedH: 52 },
  binds: ['strip'],
};

export const scaleLabelsManifest: ModuleManifest<AnyProps> = {
  id: 'scale-labels',
  title: 'Scale labels',
  category: 'display',
  component: asModule<AnyProps>(ScaleLabels),
  props: {
    variant: { type: 'enum', options: ['strip', 'aside'], default: 'strip' },
  },
  defaults: { variant: 'strip' },
  size: { minW: 28, minH: 120 },
};

const manifests: ModuleManifest<AnyProps>[] = [
  faderManifest,
  meterManifest,
  knobManifest,
  muteButtonManifest,
  panBarManifest,
  nameBarManifest,
  scaleLabelsManifest,
];

export default manifests;

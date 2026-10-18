// H1 shell package manifests (MODULES.md section 2). The L0 module system
// (ui/modules/types.ts + registry auto-load of ui/modules/manifests/*.ts)
// already exists, so these import the ModuleManifest interface directly.
// header-bar / bank-rail / scene-dropdown are presentational: props in,
// callbacks out; live values arrive via ModuleContext bindings resolved by
// the renderer from the store (fixtures now, engine in wave 3).

import type { Component } from 'svelte';
import type { ModuleManifest, ModuleProps } from '../types.ts';
import HeaderBar from '../../shell/HeaderBar.svelte';
import BankRail from '../../shell/BankRail.svelte';
import SceneButton from '../../shell/SceneButton.svelte';

function asModule<P>(c: Component): Component<ModuleProps<P>> {
  return c as unknown as Component<ModuleProps<P>>;
}

const headerBar: ModuleManifest<Record<string, never>> = {
  id: 'header-bar',
  title: 'Header bar',
  category: 'display',
  component: asModule<Record<string, never>>(HeaderBar),
  props: {},
  defaults: {},
  size: { fixedH: 56 },
  binds: ['scene', 'bank', 'setting:locked', 'setting:connected'],
};

const bankRail: ModuleManifest<Record<string, never>> = {
  id: 'bank-rail',
  title: 'Bank rail',
  category: 'display',
  component: asModule<Record<string, never>>(BankRail),
  props: {},
  defaults: {},
  size: { fixedW: 92 },
  binds: ['bank'],
};

const sceneDropdown: ModuleManifest<Record<string, never>> = {
  id: 'scene-dropdown',
  title: 'Scene dropdown',
  category: 'display',
  component: asModule<Record<string, never>>(SceneButton),
  props: {},
  defaults: {},
  size: { minW: 120, maxW: 280, fixedH: 46 },
  binds: ['scene'],
};

export const manifests: ModuleManifest[] = [headerBar, bankRail, sceneDropdown];
export default manifests;

// Presets package module manifest (picked up by loadManifestModules via
// import.meta.glob — no other wiring). One module: the channel presets
// card. No `binds`/`requires`: the card reads the fixture channel + live
// caps itself, so fixture mode renders with no desk and online mode
// annotates desk-refused leaves honestly instead of faking them.
import type { Component } from 'svelte';
import type { ModuleManifest, ModuleProps } from '../types.ts';
import PresetsCard from '../../screens/channel/PresetsCard.svelte';

function as<P>(c: unknown): Component<ModuleProps<P>> {
  return c as Component<ModuleProps<P>>;
}

export const manifests: ModuleManifest[] = [
  {
    id: 'channel-presets-card',
    title: 'Channel presets',
    category: 'channel',
    component: as(PresetsCard),
    props: {
      channelId: { type: 'text', default: '' },
    },
    defaults: { channelId: '' },
    size: { minH: 220, flex: true },
  },
];

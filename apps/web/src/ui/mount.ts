// ui=4 entry: mounts AppFrame. Packages register screens/galleries via screens/index.ts:
//   registerScreen(id, component)         -> `?ui=4&screen=<id>`
//   registerGallery(id, loader)           -> `?ui=4&gallery=<id>` (bare `?gallery` = the primitives gallery)
import { mount } from 'svelte';
import AppFrame from './AppFrame.svelte';
import type { Component } from 'svelte';

const screens = new Map<string, Component>();
const galleries = new Map<string, () => Promise<{ default: Component }>>();

/** Extension point: register a screen component by id. */
export function registerScreen(id: string, component: Component): void {
  screens.set(id, component);
}

/** Extension point: register a gallery page (lazy) by id. */
export function registerGallery(id: string, loader: () => Promise<{ default: Component }>): void {
  galleries.set(id, loader);
}

/** Resolve the screen component for URL params. */
export function screenFromParams(params: URLSearchParams): Component | null {
  if (params.has('gallery')) return null;
  const screen = params.get('screen');
  if (screen && screens.has(screen)) return screens.get(screen) as Component;
  return null;
}

export async function mountUi(target: HTMLElement, params: URLSearchParams): Promise<object> {
  const index = await import('./screens/index');
  await index.registered;
  if (params.has('gallery')) {
    const id = params.get('gallery') || 'primitives';
    const loader = galleries.get(id);
    const m = loader ? await loader() : await import('./gallery/Gallery.svelte');
    return mount(m.default, { target });
  }
  const Screen = screenFromParams(params) ?? screens.get('mixer') ?? null;
  if (Screen) return mount(Screen, { target });
  // No screen resolved and no `mixer` registered (bare package set):
  // mount the empty frame rather than failing the route.
  return mount(AppFrame, { target });
}

// ui=4 entry: mounts AppFrame. Packages register screens/galleries via screens/index.ts:
//   registerScreen(id, component)         -> `?ui=4&screen=<id>`
//   registerGallery(id, loader)           -> `?ui=4&gallery=<id>` (bare `?gallery` = the primitives gallery)
import { mount } from 'svelte';
import AppFrame from './AppFrame.svelte';
import type { Component } from 'svelte';
import { loadManifestModules } from './modules/registry.ts';
import { contentLoaderFor } from './screens/loadScreen.ts';

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

/**
 * Gallery routes need only their tiny gallery registrations (lazy component
 * loaders); no screen component is ever required on this path.
 */
function galleryLoaderFor(id: string): (() => Promise<unknown>) | null {
  switch (id) {
    case 'signal':
      return () => import('./gallery/signal/signal-register.ts');
    case 'presets':
      return () => import('./presets/presets-register.ts');
    default:
      return null;
  }
}

/**
 * Prefetch the engine modules while the paint-path fetches are still in
 * flight, so the first link-up's dynamic imports resolve from cache instead
 * of stalling connect->attach on another custom-protocol round trip (J3).
 * Fixture/gallery boots never link up, so they never pay for these chunks.
 */
function prefetchEngine(): void {
  try {
    void import('./world/engineWorld/engineWorld.ts').catch(() => undefined);
    void import('./world/transport.ts').catch(() => undefined);
    void import('@tauri-apps/api/core').catch(() => undefined);
  } catch {
    /* ignore */
  }
}

function isFixtureParams(params: URLSearchParams): boolean {
  const v = params.get('fixture');
  return v === '1' || v === 'true';
}

/** Resolve the screen component for URL params. */
export function screenFromParams(params: URLSearchParams): Component | null {
  if (params.has('gallery')) return null;
  const screen = params.get('screen');
  if (screen && screens.has(screen)) return screens.get(screen) as Component;
  return null;
}

export async function mountUi(target: HTMLElement, params: URLSearchParams): Promise<object> {
  if (params.has('gallery')) {
    const id = params.get('gallery') || 'primitives';
    const need = galleryLoaderFor(id);
    try {
      if (need) await need();
      else {
        const index = await import('./screens/index');
        await index.registered;
      }
    } catch (e) {
      // A broken package logs and the route falls back, exactly like the
      // all-settled path in screens/index.ts.
      console.error('[ui4] registration failed:', e);
    }
    // Gallery routes render outside the shell; keep the old invariant (every
    // manifest present before mount) — registration is idempotent.
    await loadManifestModules().catch((e) => {
      console.error('[ui4] manifest load failed:', e);
    });
    const loader = galleries.get(id);
    const m = loader ? await loader() : await import('./gallery/Gallery.svelte');
    const app = mount(m.default, { target });
    return app;
  }
  // Mount navigation as soon as the shell is available. Its outlet owns
  // the selected content/manifests and retry state; waiting for those here
  // would also hold the header hostage to a slow content import/profile.
  if (!isFixtureParams(params)) prefetchEngine();
  const shellP = import('./shell/shellRegister.ts');
  const screen = params.get('screen');
  const need = screen ? contentLoaderFor(screen) : contentLoaderFor(null);
  try {
    await shellP;
  } catch (e) {
    console.error('[ui4] shell registration failed:', e);
  }
  if (!need) {
    // Debug/editor/extension routes retain their full registration path.
    try {
      const index = await import('./screens/index');
      await index.registered;
    } catch (e) {
      console.error('[ui4] registration failed:', screen, e);
    }
    await loadManifestModules().catch((e) => {
      console.error('[ui4] manifest load failed:', e);
    });
  }
  const Screen = screenFromParams(params) ?? screens.get('mixer') ?? null;
  if (Screen) {
    const app = mount(Screen, { target });
    return app;
  }
  // No screen resolved and no `mixer` registered (bare package set):
  // mount the empty frame rather than failing the route.
  return mount(AppFrame, { target });
}

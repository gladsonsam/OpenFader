// ui=4 entry: mounts AppFrame. Packages register screens/galleries via screens/index.ts:
//   registerScreen(id, component)         -> `?ui=4&screen=<id>`
//   registerGallery(id, loader)           -> `?ui=4&gallery=<id>` (bare `?gallery` = the primitives gallery)
import { mount } from 'svelte';
import AppFrame from './AppFrame.svelte';
import type { Component } from 'svelte';
import { loadManifestModules } from './modules/registry.ts';
import { contentLoaderFor, prepareCoreScreens } from './screens/loadScreen.ts';

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
 * Prepare connection code behind the local readiness view so the first
 * link-up reuses cached imports. Importing does not start a connection.
 * Fixture/gallery boots never link up, so they skip this explicit preparation.
 */
async function prepareEngine(): Promise<unknown> {
  return Promise.all([
    import('./world/engineWorld/engineWorld.ts'),
    import('./world/transport.ts'),
    import('@tauri-apps/api/core'),
    import('@tauri-apps/api/event'),
  ]);
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

interface StartupOptions {
  signal?: AbortSignal;
  onProgress?: (completed: number, total: number) => void;
}

export async function mountUi(target: HTMLElement, params: URLSearchParams, options: StartupOptions = {}): Promise<object> {
  function reveal(Screen: Component): object {
    // A timed-out attempt may finish its uncancellable imports later. It must
    // never replace the retry view or a newer attempt's mounted application.
    options.signal?.throwIfAborted();
    target.replaceChildren();
    return mount(Screen, { target });
  }
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
    return reveal(m.default);
  }
  // The owner prefers a longer visible startup to a delayed first visit to
  // each page. Prepare all normal routes and controls BEFORE mounting Shell;
  // this loads code only, never connects or waits for hardware/hydration.
  const screen = params.get('screen');
  const need = screen ? contentLoaderFor(screen) : contentLoaderFor(null);
  await Promise.all([
    import('./shell/shellRegister.ts'),
    prepareCoreScreens(options.onProgress),
    import('./screens/meters/MeterBridge.svelte'),
    ...(!isFixtureParams(params) ? [prepareEngine()] : []),
  ]);
  performance.mark('openmix:screens-ready');
  if (!need && screen !== 'shell') {
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
    return reveal(Screen);
  }
  // No screen resolved and no `mixer` registered (bare package set):
  // mount the empty frame rather than failing the route.
  return reveal(AppFrame);
}

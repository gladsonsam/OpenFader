// ui=4 entry: mounts AppFrame. Packages register screens/galleries via screens/index.ts:
//   registerScreen(id, component)         -> `?ui=4&screen=<id>`
//   registerGallery(id, loader)           -> `?ui=4&gallery=<id>` (bare `?gallery` = the primitives gallery)
import { mount } from 'svelte';
import AppFrame from './AppFrame.svelte';
import type { Component } from 'svelte';
import { loadManifestModules } from './modules/registry.ts';
import { contentLoaderFor, loadShellScreen } from './screens/loadScreen.ts';

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
 * Register every remaining package without blocking paint — and without
 * stealing the paint path's connections: the burst waits for a post-mount
 * floor (two frames + 2 s) and then an idle slot, so on slow links the
 * first paint's fetches finish first (dynamic import() has no priority
 * knob; starting the burst early measurably delayed FCP on HTTP/1.1).
 * Navigation requests its own screen and manifests immediately through
 * loadShellScreen, without waiting for this background batch.
 * A watchdog covers frames that never come (hidden tab). Each package still
 * loads in isolation (screens/index.ts per-module catch), so a broken
 * package can't take down the route.
 */
function settleRemaining(): void {
  let done = false;
  const run = () => {
    if (done) return;
    done = true;
    Promise.all([
      import('./screens/index.ts').then((m) => m.registered),
      // Manifest packages deferred past paint (registration is idempotent;
      // already-loaded paint files are rejected as duplicates). Layouts
      // re-resolve via the manifest revision (see registry.ts).
      loadManifestModules(),
    ]).catch((e) => console.error('[ui4] background registration failed:', e));
  };
  const viaIdle = () => {
    try {
      const w = window as unknown as {
        requestIdleCallback?: (cb: () => void, opts?: { timeout: number }) => void;
      };
      if (typeof w.requestIdleCallback === 'function') w.requestIdleCallback(run, { timeout: 2000 });
      else setTimeout(run, 0);
    } catch {
      setTimeout(run, 0);
    }
  };
  const afterFloor = () => {
    try {
      requestAnimationFrame(() => requestAnimationFrame(() => setTimeout(viaIdle, 2000)));
      // Watchdog: frames/idle never come in a hidden tab.
      setTimeout(run, 8000);
    } catch {
      setTimeout(run, 0);
    }
  };
  afterFloor();
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
    if (need) settleRemaining();
    return app;
  }
  // The Shell (boot route) covers the default route and every design screen
  // id; mount it without awaiting the full screen graph. The shell package,
  // the content package for the requested screen, and its paint manifests
  // all fetch in parallel — the old serial awaits stalled first paint on
  // the slowest of the three. The rest settles after first paint.
  if (!isFixtureParams(params)) prefetchEngine();
  const shellP = import('./shell/shellRegister.ts');
  const screen = params.get('screen');
  const need = screen ? contentLoaderFor(screen) : contentLoaderFor(null);
  const contentP = need
    ? Promise.resolve(loadShellScreen(screen ?? 'mixer')).then(() => true)
    : import('./screens/index').then((index) => index.registered.then(() => false));
  const manifestP = (need ? Promise.resolve() : loadManifestModules()).catch((e) => {
    console.error('[ui4] manifest load failed:', e);
  });
  try {
    await shellP;
  } catch (e) {
    console.error('[ui4] shell registration failed:', e);
  }
  try {
    await contentP;
  } catch (e) {
    // A broken package logs and the route falls back, exactly like the
    // all-settled path in screens/index.ts.
    console.error('[ui4] registration failed:', screen, e);
  }
  await manifestP;
  const Screen = screenFromParams(params) ?? screens.get('mixer') ?? null;
  if (Screen) {
    const app = mount(Screen, { target });
    if (need) settleRemaining();
    return app;
  }
  // No screen resolved and no `mixer` registered (bare package set):
  // mount the empty frame rather than failing the route.
  return mount(AppFrame, { target });
}

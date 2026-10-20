// ui=4 entry: mounts AppFrame. Packages register screens/galleries via screens/index.ts:
//   registerScreen(id, component)         -> `?ui=4&screen=<id>`
//   registerGallery(id, loader)           -> `?ui=4&gallery=<id>` (bare `?gallery` = the primitives gallery)
import { mount } from 'svelte';
import AppFrame from './AppFrame.svelte';
import type { Component } from 'svelte';
import { loadManifestModules } from './modules/registry.ts';

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
 * Content packages needed for first paint, by mount id. Each loader imports
 * ONLY its package's register file (which pulls that screen's component);
 * every other package settles in the background after mount (see
 * settleRemaining). Returns null for ids owned by debug/demo/editor routes,
 * which fall through to full registration like before.
 */
function contentLoaderFor(screen: string | null): (() => Promise<unknown>) | null {
  switch (screen) {
    case null: // default route boots the mixer content
    case 'mixer':
      return () => import('./screens/mixer/register.ts');
    case 'ch-input':
    case 'ch-eq':
    case 'ch-gate':
    case 'ch-comp':
    case 'ch-sends':
    case 'ch-groups':
      return () =>
        Promise.all([
          import('./screens/channel/register.ts'),
          import('./screens/channel/eq/register.ts'),
          import('./screens/channel/dyn/register.ts'),
        ]);
    case 'fx':
      return () => import('./screens/fx/register.ts');
    case 'routing':
      return () => import('./screens/routing/register.ts');
    case 'connection':
      return () => import('./screens/connection/register.ts');
    case 'settings':
      return () => import('./screens/settings/register.ts');
    case 'scenes':
      return () => import('./screens/scenes/register.ts');
    default:
      return null;
  }
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
 * Late registrations bump the shell revision so an already-mounted screen
 * that rendered a placeholder picks up its content (see shell/registry.ts).
 * A watchdog covers frames that never come (hidden tab). Each package still
 * loads in isolation (screens/index.ts per-module catch), so a broken
 * package can't take down the route.
 */
function settleRemaining(): void {
  let done = false;
  const run = () => {
    if (done) return;
    done = true;
    import('./screens/index.ts')
      .then((m) => m.registered)
      .catch((e) => console.error('[ui4] background registration failed:', e));
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
 * Layout screens (mixer/channel/…) render through LayoutRenderer, which
 * reads the module manifest registry — a plain non-reactive Map populated
 * by loadManifestModules(). The old boot awaited full registration (which
 * runs it via the demo/editor packages) before mounting anything, so every
 * route could assume manifests were present at first render. The lazy boot
 * defers those packages, so restore the invariant explicitly — before
 * mounting, on every route. The manifest modules ride the boot graph (shared
 * with the screen components); registration itself is synchronous and
 * idempotent.
 */
function ensureManifests(): void {
  try {
    loadManifestModules();
  } catch (e) {
    console.error('[ui4] manifest load failed:', e);
  }
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
    ensureManifests();
    const loader = galleries.get(id);
    const m = loader ? await loader() : await import('./gallery/Gallery.svelte');
    const app = mount(m.default, { target });
    if (need) settleRemaining();
    return app;
  }
  // The Shell (boot route) covers the default route and every design screen
  // id; mount it without awaiting the full screen graph. Only the content
  // package for the requested screen is awaited — the rest settles after
  // first paint.
  await import('./shell/shellRegister.ts');
  const screen = params.get('screen');
  const need = screen ? contentLoaderFor(screen) : contentLoaderFor(null);
  if (need) {
    try {
      await need();
    } catch (e) {
      console.error('[ui4] registration failed:', screen, e);
    }
  } else {
    // Non-design screen (world-debug, l0-demo, layout-editor, …): wait for
    // every package, exactly like before, then resolve.
    const index = await import('./screens/index');
    await index.registered;
  }
  ensureManifests();
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

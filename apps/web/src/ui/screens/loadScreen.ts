// Shared by boot and in-app navigation: a screen is ready only after both
// its component and its layout modules have registered. Unselected screens
// are warmed in bounded post-paint work by preloadScreens.ts.
import type { Component } from 'svelte';
import { loadManifestModules, manifestModuleFiles } from '../modules/registry.ts';
import { getShellScreen } from '../shell/registry.ts';
import { SCREEN_IDS } from '../state/urlBridge.ts';
import { loadStoredProfile } from '../modules/profile/io.ts';

/**
 * Content packages needed for first paint, by mount id. Each loader imports
 * ONLY its package's register file (which pulls that screen's component);
 * every other package settles in the background after mount (see
 * preloadScreens). Returns null for ids owned by debug/demo/editor routes,
 * which fall through to full registration like before.
 */
export function contentLoaderFor(screen: string | null): (() => Promise<unknown>) | null {
  switch (screen) {
    case null: // default route boots the mixer content
    case 'mixer':
      return () => import('./mixer/register.ts');
    case 'ch-input':
    case 'ch-eq':
    case 'ch-gate':
    case 'ch-comp':
    case 'ch-sends':
    case 'ch-groups':
      return () =>
        Promise.all([
          import('./channel/register.ts'),
          import('./channel/eq/register.ts'),
          import('./channel/dyn/register.ts'),
        ]);
    case 'fx':
      return () => import('./fx/register.ts');
    case 'routing':
      return () => import('./routing/register.ts');
    case 'connection':
      return () => import('./connection/register.ts');
    case 'settings':
      return () => import('./settings/register.ts');
    case 'scenes':
      return () => import('./scenes/register.ts');
    default:
      return null;
  }
}

/**
 * Manifest packages needed for first paint, by mount id (glob keys relative
 * to modules/registry.ts). The mixer paint needs only the layout primitives
 * (`_example`, incl. the generic `btn` leaf) + the strip modules
 * (`m1-mixer`); every other package settles in the background after mount
 * (see preloadScreens) and LayoutRenderer re-resolves when it lands.
 * Returns null for ids owned by debug/demo/editor routes, which fall
 * through to full registration like before.
 */
function manifestsFor(screen: string | null): string[] | null {
  switch (screen) {
    case null: // default route boots the mixer content
    case 'mixer':
      return ['./manifests/_example.ts', './manifests/m1-mixer.ts'];
    case 'ch-input':
    case 'ch-eq':
    case 'ch-gate':
    case 'ch-comp':
    case 'ch-sends':
    case 'ch-groups':
      return [
        './manifests/_example.ts',
        './manifests/c1-channel.ts',
        './manifests/c2-eq.ts',
        './manifests/c3-dyn.ts',
        './manifests/p1-presets.ts',
        './manifests/m1-mixer.ts',
      ];
    case 'fx':
      return ['./manifests/_example.ts', './manifests/f1-fx.ts'];
    case 'routing':
    case 'connection':
      return ['./manifests/_example.ts', './manifests/s78-pages.ts'];
    case 'settings':
    case 'scenes':
      return ['./manifests/_example.ts', './manifests/s910-pages.ts'];
    default:
      return null;
  }
}

const ready = new Map<string, Component>();
const pending = new Map<string, Promise<Component>>();
const contentLoads = new Map<string, Promise<unknown>>();
let shellPaint: Promise<void> | undefined;

function afterShellPaint(): Promise<void> {
  if (shellPaint) return shellPaint;
  // Even cached dynamic imports can resolve in the mount's microtask
  // flush and render a large mixer before the header ever paints. Release
  // the initial outlet only after a paint opportunity; subsequent routes
  // reuse the settled promise and load immediately.
  shellPaint = new Promise(resolve => {
    if (typeof window === 'undefined' || typeof window.requestAnimationFrame !== 'function') {
      resolve();
      return;
    }
    let frame: number;
    const timer = setTimeout(done, 250);
    function done(): void {
      clearTimeout(timer);
      window.cancelAnimationFrame(frame);
      resolve();
    }
    frame = window.requestAnimationFrame(() => {
      frame = window.requestAnimationFrame(done);
    });
  });
  return shellPaint;
}

export function shellScreenReady(id: string): boolean {
  return ready.has(id);
}

function loadContent(id: string): Promise<unknown> {
  // Channel tabs share one component/registration package.
  const key = id.startsWith('ch-') ? 'channel' : id;
  const cached = contentLoads.get(key);
  if (cached) return cached;
  const load = contentLoaderFor(id);
  if (!load) return Promise.reject(new Error(`Unknown screen: ${id}`));
  const request = load().catch(error => {
    contentLoads.delete(key);
    throw error;
  });
  contentLoads.set(key, request);
  return request;
}

function requiredManifests(id: string): string[] {
  // Profiles may insert modules from ANY package. Only customized routes
  // need the full palette before rendering; ordinary boot stays lean.
  const profile = id.startsWith('ch-') ? 'channel' : id;
  if (loadStoredProfile(profile)?.ops.length) return manifestModuleFiles();
  return manifestsFor(id) ?? [];
}

function checkManifests(results: Awaited<ReturnType<typeof loadManifestModules>>): void {
  const errors = results.flatMap(r => r.errors).filter(e => !e.includes('duplicate module id:'));
  if (errors.length) throw new Error(errors.join('; '));
}

export function screenNavigationPending(): boolean {
  return pending.size > 0;
}

/** Each task performs one import unit. No components are instantiated and
 * no component effects run. Navigation uses the same caches immediately,
 * without ever waiting for the idle queue to finish this route. */
export function screenPreloadTasks(id: string): Array<() => Promise<unknown>> {
  // This plan is created lazily in an idle turn. Reuse its validated
  // profile/dependency decision when finalizing, rather than parsing the
  // same profile again after its imports complete.
  const files = requiredManifests(id);
  return [
    () => loadContent(id),
    ...files.map(file => async () => checkManifests(await loadManifestModules([file]))),
    async () => { await ensureShellScreen(id, files); },
  ];
}

/** Cached screens return synchronously so reopening never flashes loading. */
export function loadShellScreen(id: string): Component | Promise<Component> {
  return ensureShellScreen(id);
}

function ensureShellScreen(id: string, files?: string[]): Component | Promise<Component> {
  const cached = ready.get(id);
  if (cached) return cached;
  const inFlight = pending.get(id);
  if (inFlight) return inFlight;
  const request = afterShellPaint().then(() => Promise.all([
    loadContent(id),
    loadManifestModules(files ?? requiredManifests(id)),
  ])).then(([, results]) => {
    // Overlapping boot/background/navigation loads may register the same
    // modules. Duplicates are expected; actual import/validation failures
    // must reach the outlet's retry state instead of missing-module chrome.
    checkManifests(results);
    const screen = getShellScreen(id);
    if (!screen) throw new Error(`Screen did not register: ${id}`);
    ready.set(id, screen);
    // All channel tabs use the same component and manifest dependency set.
    // Warming input therefore also makes the first EQ/dynamics/sends open
    // synchronous, instead of spending extra idle turns on cached imports.
    if (id.startsWith('ch-')) {
      for (const alias of SCREEN_IDS.filter(value => value.startsWith('ch-'))) {
        const component = getShellScreen(alias);
        if (component) ready.set(alias, component);
      }
    }
    pending.delete(id);
    return screen;
  }).catch(error => {
    pending.delete(id);
    throw error;
  });
  pending.set(id, request);
  return request;
}

// Shared by boot and in-app navigation: a screen is ready only after both
// its component and its layout modules have registered. Every normal route
// is prepared behind the startup view before the shell is revealed.
import type { Component } from 'svelte';
import { loadManifestModules, manifestModuleFiles } from '../modules/registry.ts';
import { getShellScreen } from '../shell/registry.ts';
import { SCREEN_IDS } from '../state/urlBridge.ts';
import { loadStoredProfile } from '../modules/profile/io.ts';

/**
 * Content packages by mount id, shared by startup and navigation.
 * Returns null for ids owned by debug/demo/editor routes,
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

/** Required default layout/widget packages by route. Custom profiles may
 * reference any registered module, so they use the complete manifest palette.
 * Debug/demo/editor routes retain the full-registration fallback. */
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
  // Profiles may insert modules from ANY package. Customized routes need
  // the full palette before rendering, including at startup.
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

/** Prepare every normal destination before revealing the app. No hidden
 * components mount, no connection starts, and no fixture data is substituted.
 * Channel routes share one dependency/profile plan; all aliases are marked ready
 * only after that entire plan succeeds. Custom layouts receive the full palette.
 */
export async function prepareCoreScreens(onProgress?: (completed: number, total: number) => void): Promise<void> {
  onProgress?.(SCREEN_IDS.filter(shellScreenReady).length, SCREEN_IDS.length);
  const routes = SCREEN_IDS.filter(id => !id.startsWith('ch-') || id === 'ch-input');
  await Promise.all(routes.map(async id => {
    await ensureShellScreen(id);
    onProgress?.(SCREEN_IDS.filter(shellScreenReady).length, SCREEN_IDS.length);
  }));
  const missing = SCREEN_IDS.filter(id => !shellScreenReady(id));
  if (missing.length) throw new Error(`Screens did not register: ${missing.join(', ')}`);
}

/** Cached screens return synchronously so reopening never flashes loading. */
export function loadShellScreen(id: string): Component | Promise<Component> {
  return ensureShellScreen(id);
}

function ensureShellScreen(id: string): Component | Promise<Component> {
  const cached = ready.get(id);
  if (cached) return cached;
  const key = id.startsWith('ch-') ? 'channel' : id;
  const inFlight = pending.get(key);
  if (inFlight) return inFlight;
  const request = Promise.resolve().then(() => Promise.all([
    loadContent(id),
    loadManifestModules(requiredManifests(id)),
  ])).then(([, results]) => {
    // Overlapping boot/background/navigation loads may register the same
    // modules. Duplicates are expected; actual import/validation failures
    // must reach the outlet's retry state instead of missing-module chrome.
    checkManifests(results);
    const screen = getShellScreen(id);
    if (!screen) throw new Error(`Screen did not register: ${id}`);
    ready.set(id, screen);
    // All channel tabs use the same component and manifest dependency set.
    // Preparing input therefore makes the first EQ/dynamics/sends open
    // synchronous, without repeating the shared profile/dependency plan.
    if (id.startsWith('ch-')) {
      for (const alias of SCREEN_IDS.filter(value => value.startsWith('ch-'))) {
        const component = getShellScreen(alias);
        if (component) ready.set(alias, component);
      }
    }
    pending.delete(key);
    return screen;
  }).catch(error => {
    pending.delete(key);
    throw error;
  });
  pending.set(key, request);
  return request;
}

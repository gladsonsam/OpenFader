// Shared by boot and in-app navigation: a screen is ready only after both
// its component and its layout modules have registered. Unselected screens
// retain the existing post-paint background loading policy.
import type { Component } from 'svelte';
import { loadManifestModules } from '../modules/registry.ts';
import { getShellScreen } from '../shell/registry.ts';

/**
 * Content packages needed for first paint, by mount id. Each loader imports
 * ONLY its package's register file (which pulls that screen's component);
 * every other package settles in the background after mount (see
 * settleRemaining). Returns null for ids owned by debug/demo/editor routes,
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
 * (see settleRemaining) and LayoutRenderer re-resolves when it lands.
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

/** Cached screens return synchronously so reopening never flashes loading. */
export function loadShellScreen(id: string): Component | Promise<Component> {
  const cached = ready.get(id);
  if (cached) return cached;
  const inFlight = pending.get(id);
  if (inFlight) return inFlight;
  const load = contentLoaderFor(id);
  const files = manifestsFor(id);
  const request = Promise.all([
    load ? load() : Promise.reject(new Error(`Unknown screen: ${id}`)),
    loadManifestModules(files ?? []),
  ]).then(([, results]) => {
    // Overlapping boot/background/navigation loads may register the same
    // modules. Duplicates are expected; actual import/validation failures
    // must reach the outlet's retry state instead of missing-module chrome.
    const errors = results.flatMap(r => r.errors).filter(e => !e.includes('duplicate module id:'));
    if (errors.length) throw new Error(errors.join('; '));
    const screen = getShellScreen(id);
    if (!screen) throw new Error(`Screen did not register: ${id}`);
    ready.set(id, screen);
    pending.delete(id);
    return screen;
  }).catch(error => {
    pending.delete(id);
    throw error;
  });
  pending.set(id, request);
  return request;
}

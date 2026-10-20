// H1 shell package: content-screen registry (the router outlet).
//
// The Shell renders HeaderBar + the registered screen for the store's
// current mode/tab + the bank rail (mixer only). Screen packages that land
// later (M1 mixer, C1-C3 channel, F1 fx, S7-S10 pages) plug their content
// in via registerShellScreen(id, component) or the bulk registerDesignScreens
// helper — they never touch the header or rail. Until a screen registers,
// the Shell shows a neutral `screen not built yet: <id>` placeholder.
//
// Screen ids are the fixture URL contract (see ui/state/urlBridge.ts):
// mixer | ch-input | ch-eq | ch-gate | ch-comp | ch-sends |
// fx | routing | connection | settings | scenes.

import type { Component } from 'svelte';

const shellScreens = new Map<string, Component>();

/** Register one content screen for the shell outlet. Overwrites on re-register. */
export function registerShellScreen(id: string, component: Component): void {
  shellScreens.set(id, component);
  notifyShellScreens();
}

/**
 * Bulk plug-in for screen packages: registerDesignScreens({ mixer: MixerScreen,
 * 'ch-eq': EqTab, ... }). Exported so later packages' screens plug in without
 * touching shell files.
 */
export function registerDesignScreens(map: Record<string, Component>): void {
  for (const [id, component] of Object.entries(map)) registerShellScreen(id, component);
}

/** Look up the outlet component for a design screen id (null = placeholder). */
export function getShellScreen(id: string): Component | null {
  return shellScreens.get(id) ?? null;
}

/**
 * Revision bumped on every shell-screen registration. Boot mounts the Shell
 * before all content packages finish (see ui/mount.ts); the Shell
 * subscribes so a late registration re-resolves instead of sticking on the
 * `screen not built yet` placeholder.
 */
let revision = 0;
const listeners = new Set<() => void>();

export function shellScreensRevision(): number {
  return revision;
}

export function subscribeShellScreens(fn: () => void): () => void {
  listeners.add(fn);
  return () => {
    listeners.delete(fn);
  };
}

function notifyShellScreens(): void {
  revision++;
  for (const fn of [...listeners]) {
    try {
      fn();
    } catch {
      /* ignore: listener-local failure */
    }
  }
}

/** Remove all registrations (node tests only). */
export function clearShellScreens(): void {
  shellScreens.clear();
  notifyShellScreens();
}

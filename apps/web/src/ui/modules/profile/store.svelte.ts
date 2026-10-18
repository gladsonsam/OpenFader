// L0 profile store (Svelte 5 runes): current UserProfile over shipped
// defaults, persisted to localStorage (`openmix.profile.v2.<id>`), with pure
// undo/redo over the op list. Locked profiles reject mutations.

import { applyOp, applyProfile, type ProfileOp } from './ops.ts';
import { emptyProfile, loadStoredProfile, saveStoredProfile, type UserProfile } from './io.ts';
import { createHistory, pushHistory, redoHistory, undoHistory } from './history.ts';
import { cloneNode } from '../schema.ts';
import type { LayoutNode } from '../schema.ts';

export interface ProfileStore {
  readonly profile: UserProfile;
  readonly skipped: string[];
  tree(defaults: LayoutNode): LayoutNode;
  commit(op: ProfileOp): boolean;
  removeOp(index: number): boolean;
  reset(): void;
  replace(next: UserProfile): boolean;
  undo(): boolean;
  redo(): boolean;
  readonly canUndo: boolean;
  readonly canRedo: boolean;
  setLocked(locked: boolean): void;
  setThemeVar(name: string, value: string): void;
  setCustomCss(css: string): void;
}

export function createProfileStore(id: string, fallback?: UserProfile): ProfileStore {
  let profile = $state<UserProfile>(loadStoredProfile(id) ?? fallback ?? emptyProfile(id));
  let hist = $state(createHistory<ProfileOp[]>(profile.ops));

  function persist(): void {
    profile.ops = hist.present;
    saveStoredProfile(profile);
  }

  return {
    get profile(): UserProfile {
      return profile;
    },
    get skipped(): string[] {
      return [];
    },
    tree(defaults: LayoutNode): LayoutNode {
      return applyProfile(defaults, { ops: hist.present, locked: profile.locked }).tree;
    },
    commit(op: ProfileOp): boolean {
      if (profile.locked) return false;
      const probe = cloneNode(hist.present);
      void probe;
      // Validate against a dummy tree is not possible here; lock is the
      // store-level guard — structural validation happens in applyOp at render.
      hist = pushHistory(hist, [...hist.present, op]);
      persist();
      return true;
    },
    removeOp(index: number): boolean {
      if (profile.locked) return false;
      if (index < 0 || index >= hist.present.length) return false;
      hist = pushHistory(hist, hist.present.filter((_, i) => i !== index));
      persist();
      return true;
    },
    /** Replace the whole profile document (import path). Rejected when locked. */
    replace(next: UserProfile): boolean {
      if (profile.locked) return false;
      profile = { ...cloneNode(next), id: profile.id };
      hist = createHistory<ProfileOp[]>(profile.ops);
      saveStoredProfile(profile);
      return true;
    },
    reset(): void {
      if (profile.locked) return;
      hist = pushHistory(hist, []);
      persist();
    },
    undo(): boolean {
      const next = undoHistory(hist);
      if (next === hist) return false;
      hist = next;
      persist();
      return true;
    },
    redo(): boolean {
      const next = redoHistory(hist);
      if (next === hist) return false;
      hist = next;
      persist();
      return true;
    },
    get canUndo(): boolean {
      return hist.past.length > 0;
    },
    get canRedo(): boolean {
      return hist.future.length > 0;
    },
    setLocked(locked: boolean): void {
      profile.locked = locked;
      saveStoredProfile(profile);
    },
    setThemeVar(name: string, value: string): void {
      if (profile.locked) return;
      profile.theme = { ...profile.theme, [name]: value };
      saveStoredProfile(profile);
    },
    setCustomCss(css: string): void {
      if (profile.locked) return;
      profile.customCss = css;
      saveStoredProfile(profile);
    },
  };
}

/** Single shared store for the L0 demo (real screens own one per profile id). */
export let demoStore: ProfileStore | null = null;

export function getDemoStore(): ProfileStore {
  if (!demoStore) demoStore = createProfileStore('l0-demo');
  return demoStore;
}

// Re-exported so renderers can apply single ops with lock semantics in one call.
export { applyOp };
export type { ProfileOp };

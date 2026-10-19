// Visual-editor edit-mode state (Svelte 5 runes): whether the layout canvas
// is editable, which node is selected, and which role previews the layout.
// Pure gating helpers live here too so screens and tests share one rule:
//
//   canEditProfile(locked, role) = !locked && role !== 'musician'
//
// Engineers get full edit (insert/move/remove/reset + banks/pages/share).
// Operators get the same canvas ops except remove (they can hide instead).
// Musicians never see Edit mode (their layout is fixed by the profile).

import type { Role } from '../types.ts';

/** Full edit allowed (locked profiles reject every mutation; see ops.ts). */
export function canEditProfile(locked: boolean, role: Role): boolean {
  if (locked) return false;
  return role !== 'musician';
}

/** Destructive remove is engineer-only; operators hide instead. */
export function canRemoveNode(role: Role): boolean {
  return role === 'engineer';
}

/** Banks/pages/share editing is engineer-only. */
export function canEditLibrary(role: Role): boolean {
  return role === 'engineer';
}

export interface EditState {
  /** Edit mode on (canvas chrome visible). Forced off when !canEditProfile. */
  enabled: boolean;
  /** Selected node id (inspector + outline highlight). */
  selectedId: string | null;
  /** Preview/role gate for the canvas. */
  role: Role;
}

export function createEditState(initialRole: Role = 'engineer'): EditState {
  const s = $state<EditState>({ enabled: false, selectedId: null, role: initialRole });
  return s;
}

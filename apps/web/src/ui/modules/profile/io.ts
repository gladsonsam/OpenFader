// L0 profile document: versioned overrides over shipped defaults.
// Stored locally under `openmix.profile.v2.<id>`; exportable/importable JSON.
// Validation never throws. Migration chain lives in migrate.ts.

import type { ProfileOp } from './ops.ts';
import { validateOp } from './ops.ts';
import { migrateProfile } from './migrate.ts';

export const PROFILE_V = 2;
export const PROFILE_LS_PREFIX = 'openmix.profile.v2';

export type TokenOverrides = Record<string, string>;

export interface BankDef {
  id: string;
  name: string;
  items: string[];
  color?: string;
}

/** Built-in nav placements for pages. */
export type NavPlacement = 'header' | 'rail' | 'tab' | 'hidden';

export interface PageDef {
  id: string;
  title: string;
  /** Layout id in ui/layouts (e.g. a *.default.json stem). */
  layout: string;
  nav: NavPlacement;
  hidden?: boolean;
}

/**
 * Key set = the design's Settings screen: strips per screen, peak hold,
 * fine, sends-on-fader, mute enable, screen lock, scene recall mode.
 */
export interface BehaviourSettings {
  stripsPerScreen?: number;
  peakHold?: boolean;
  fine?: boolean;
  sendsOnFader?: boolean;
  muteEnable?: boolean;
  locked?: boolean;
  sceneConfirm?: boolean;
}

export interface UserProfile {
  v: number;
  id: string;
  ops: ProfileOp[];
  theme: TokenOverrides;
  customCss?: string;
  banks: BankDef[];
  settings: BehaviourSettings;
  pages: PageDef[];
  /** Role lock: locked profiles reject mutations and hide Edit mode. */
  locked: boolean;
}

export function emptyProfile(id: string): UserProfile {
  return { v: PROFILE_V, id, ops: [], theme: {}, banks: [], settings: {}, pages: [], locked: false };
}

function isRecord(v: unknown): v is Record<string, unknown> {
  return typeof v === 'object' && v !== null && !Array.isArray(v);
}

export interface ProfileValidation {
  ok: boolean;
  errors: string[];
}

/** Validate a parsed profile document. Never throws. */
export function validateProfile(input: unknown): ProfileValidation {
  try {
    const errors: string[] = [];
    if (!isRecord(input)) return { ok: false, errors: ['profile must be an object'] };
    if (typeof input['id'] !== 'string' || !input['id']) errors.push('id must be a non-empty string');
    if (typeof input['locked'] !== 'boolean') errors.push('locked must be a boolean');
    if (!Array.isArray(input['ops'])) errors.push('ops must be an array');
    else {
      (input['ops'] as unknown[]).forEach((o, i) => {
        const r = validateOp(o);
        if (!r.ok) errors.push(`ops[${i}]: ${(r as { error: string }).error}`);
      });
    }
    if (!isRecord(input['theme'])) errors.push('theme must be an object');
    else {
      for (const [k, v] of Object.entries(input['theme'])) {
        if (typeof v !== 'string') errors.push(`theme.${k} must be a string`);
      }
    }
    if (input['customCss'] !== undefined && typeof input['customCss'] !== 'string') {
      errors.push('customCss must be a string');
    }
    if (!Array.isArray(input['banks'])) errors.push('banks must be an array');
    else {
      (input['banks'] as unknown[]).forEach((b, i) => {
        if (!isRecord(b) || typeof b['id'] !== 'string' || !Array.isArray(b['items'])) {
          errors.push(`banks[${i}] must be { id, name, items[] }`);
        }
      });
    }
    if (!isRecord(input['settings'])) errors.push('settings must be an object');
    if (!Array.isArray(input['pages'])) errors.push('pages must be an array');
    else {
      (input['pages'] as unknown[]).forEach((p, i) => {
        if (!isRecord(p) || typeof p['id'] !== 'string' || typeof p['layout'] !== 'string') {
          errors.push(`pages[${i}] must be { id, title, layout, nav }`);
        } else if (!['header', 'rail', 'tab', 'hidden'].includes(p['nav'] as string)) {
          errors.push(`pages[${i}].nav must be header|rail|tab|hidden`);
        }
      });
    }
    return { ok: errors.length === 0, errors };
  } catch (err) {
    return { ok: false, errors: [err instanceof Error ? err.message : 'invalid profile'] };
  }
}

/** Serialise a profile to exportable JSON text. */
export function exportProfile(profile: UserProfile): string {
  return JSON.stringify({ ...profile, v: PROFILE_V }, null, 2);
}

export type ImportResult = { ok: true; profile: UserProfile } | { ok: false; errors: string[] };

/** Parse + migrate + validate exported JSON. Never throws; garbage rejected. */
export function importProfile(json: string): ImportResult {
  try {
    let parsed: unknown;
    try {
      parsed = JSON.parse(json);
    } catch {
      return { ok: false, errors: ['invalid JSON'] };
    }
    const migrated = migrateProfile(parsed);
    if (!migrated.ok) return { ok: false, errors: migrated.errors };
    const v = validateProfile(migrated.profile);
    if (!v.ok) return { ok: false, errors: v.errors };
    return { ok: true, profile: migrated.profile as UserProfile };
  } catch (err) {
    return { ok: false, errors: [err instanceof Error ? err.message : 'invalid profile'] };
  }
}

function storage(): Storage | null {
  try {
    return typeof localStorage === 'undefined' ? null : localStorage;
  } catch {
    return null;
  }
}

export function profileKey(id: string): string {
  return `${PROFILE_LS_PREFIX}.${id}`;
}

/** Load a stored profile (migrated + validated); null when absent/unreadable. */
export function loadStoredProfile(id: string): UserProfile | null {
  const s = storage();
  if (!s) return null;
  try {
    const raw = s.getItem(profileKey(id));
    if (!raw) return null;
    const r = importProfile(raw);
    return r.ok ? r.profile : null;
  } catch {
    return null;
  }
}

export function saveStoredProfile(profile: UserProfile): void {
  const s = storage();
  if (!s) return;
  try {
    s.setItem(profileKey(profile.id), exportProfile(profile));
  } catch {
    /* storage full/blocked: simply don't persist */
  }
}

export function clearStoredProfile(id: string): void {
  const s = storage();
  if (!s) return;
  try {
    s.removeItem(profileKey(id));
  } catch {
    /* ignore */
  }
}

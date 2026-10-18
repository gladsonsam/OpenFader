// User channel presets: local persistence (UI-local storage).
//
// Saved under `openmix.channel-presets.v1` (same `openmix.*` localStorage
// convention as the relay host + layout profiles). Engine support is NOT
// needed: a user preset is a fixture channel snapshot plus recall through
// the live write path — the desk only sees ordinary per-leaf edits.
// Shape errors never throw: corrupt entries are dropped, corrupt stores
// read as empty.

import { sanitizePreset, type ChannelPreset } from './channelPresets.ts';

export const USER_PRESETS_LS_KEY = 'openmix.channel-presets.v1';
const MAX_USER_PRESETS = 64;

type StorageLike = Pick<Storage, 'getItem' | 'setItem' | 'removeItem'> | null | undefined;

function storageOf(s?: StorageLike): StorageLike {
  if (s !== undefined) return s;
  try {
    return typeof localStorage === 'undefined' ? null : localStorage;
  } catch {
    return null;
  }
}

/** Load + validate the user library (empty when missing/corrupt). */
export function loadUserPresets(from?: StorageLike): ChannelPreset[] {
  const ls = storageOf(from);
  if (!ls) return [];
  let raw: string | null = null;
  try {
    raw = ls.getItem(USER_PRESETS_LS_KEY);
  } catch {
    return [];
  }
  if (!raw) return [];
  try {
    const parsed: unknown = JSON.parse(raw);
    const list = (parsed as { presets?: unknown })?.presets;
    if (!Array.isArray(list)) return [];
    const out: ChannelPreset[] = [];
    for (const item of list.slice(0, MAX_USER_PRESETS)) {
      const p = sanitizePreset(item, 'user');
      if (p) out.push(p);
    }
    return out;
  } catch {
    return [];
  }
}

/** Persist the user library (best-effort; quota errors are swallowed). */
export function persistUserPresets(list: ChannelPreset[], to?: StorageLike): void {
  const ls = storageOf(to);
  if (!ls) return;
  try {
    ls.setItem(USER_PRESETS_LS_KEY, JSON.stringify({ version: 1, presets: list.slice(0, MAX_USER_PRESETS) }));
  } catch {
    /* ignore: storage full or unavailable */
  }
}

/** Insert or replace by id (pure; caller persists). Newest first. */
export function upsertUserPreset(list: ChannelPreset[], preset: ChannelPreset): ChannelPreset[] {
  const rest = list.filter(p => p.id !== preset.id);
  return [{ ...preset, source: 'user' as const }, ...rest].slice(0, MAX_USER_PRESETS);
}

/** Remove by id (pure; caller persists). */
export function deleteUserPreset(list: ChannelPreset[], id: string): ChannelPreset[] {
  return list.filter(p => p.id !== id);
}

/** Rename by id; blank names are refused (null). Pure; caller persists. */
export function renameUserPreset(list: ChannelPreset[], id: string, name: string): ChannelPreset[] | null {
  const label = name.trim().slice(0, 32);
  if (!label) return null;
  let changed = false;
  const out = list.map(p => {
    if (p.id !== id) return p;
    changed = true;
    return { ...p, label };
  });
  return changed ? out : list;
}

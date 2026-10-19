// W2a param store: caps index + confirmed values + optimistic pending edits.
//
// Adapted from the old v3 state's paramLogic.ts (the proven MS2 logic:
// optimistic set, confirm-into-pending, timeout revert, focus normalize —
// copied with attribution, not imported: the v3 tree was deleted at the ui4
// cutover). Differences: caps rows carry `default` (engine wire name) and
// `scopes` (v2 strip-kind set); `coerceWrite` clamps floats into range and
// rounds enums; focus keeps MRU-first ordering for the EngineWorld.
//
// Pure (no DOM, no runes): node-testable via engineWorld.test.ts with an
// injected clock.

import type { ParamValue } from '../types.ts';
import type { CapsParamEvt } from '../protocol.ts';

/** One indexed caps row (ranges in engineering units, never wire floats). */
export interface CapsRow {
  id: string;
  kind: string;
  min: number;
  max: number;
  unit: string;
  def: ParamValue;
  label: string;
  scopes: string[];
}

/** Indexed capabilities: `ready` is false until the first caps frame lands. */
export interface CapsSnapshot {
  ready: boolean;
  driver: string;
  defs: Record<string, CapsRow>;
  supported: string[];
  unsupported: string[];
}

/** Optimistic-write confirmation window before a revert. */
export const CONFIRM_TIMEOUT_MS = 2000;
/** Focus sends coalesce rapid scroll/layer changes into one frame. */
export const FOCUS_DEBOUNCE_MS = 150;
/** SPEC 2a write coalescing: at most 60 sends/s per control after the edge. */
export const WRITE_MIN_INTERVAL_MS = 1000 / 60;

export function emptyCaps(): CapsSnapshot {
  return { ready: false, driver: '', defs: {}, supported: [], unsupported: [] };
}

/** Index a caps frame. Unknown/malformed entries are skipped. */
export function capsFromInput(input: {
  driver?: unknown;
  params?: CapsParamEvt[] | unknown;
  unsupported?: unknown;
}): CapsSnapshot {
  const defs: Record<string, CapsRow> = {};
  const supported: string[] = [];
  if (input && Array.isArray(input.params)) {
    for (const p of input.params as CapsParamEvt[]) {
      if (p === null || typeof p !== 'object' || Array.isArray(p)) continue;
      const rec = p as unknown as Record<string, unknown>;
      if (typeof rec['id'] !== 'string' || !rec['id']) continue;
      if (defs[rec['id'] as string] !== undefined) continue;
      const num = (v: unknown): number => (typeof v === 'number' && Number.isFinite(v) ? v : 0);
      const raw = rec['default'];
      const def: ParamValue =
        typeof raw === 'string' || typeof raw === 'boolean'
          ? raw
          : typeof raw === 'number' && Number.isFinite(raw)
            ? raw
            : 0;
      const scopesRaw = rec['scopes'];
      defs[rec['id'] as string] = {
        id: rec['id'] as string,
        kind: typeof rec['kind'] === 'string' ? (rec['kind'] as string) : '',
        min: num(rec['min']),
        max: num(rec['max']),
        unit: typeof rec['unit'] === 'string' ? (rec['unit'] as string) : '',
        def,
        label: typeof rec['label'] === 'string' ? (rec['label'] as string) : (rec['id'] as string),
        scopes: Array.isArray(scopesRaw) ? scopesRaw.filter((s): s is string => typeof s === 'string') : [],
      };
      supported.push(rec['id'] as string);
    }
  }
  const unsupported = Array.isArray(input?.unsupported)
    ? (input.unsupported as unknown[]).filter((s): s is string => typeof s === 'string' && !!s)
    : [];
  return {
    ready: true,
    driver: typeof input?.driver === 'string' ? (input.driver as string) : '',
    defs,
    supported,
    unsupported,
  };
}

/** Confirmed per-channel values (channel -> id -> engineering value). */
export type ChannelValues = Record<number, Record<string, ParamValue>>;

export interface PendingEdit {
  value: ParamValue;
  at: number;
}

/** Optimistic edits awaiting confirmation, keyed `${ch}:${id}`. */
export type PendingMap = Record<string, PendingEdit>;

export interface StoreMaps {
  confirmed: ChannelValues;
  pending: PendingMap;
}

export function emptyMaps(): StoreMaps {
  return { confirmed: {}, pending: {} };
}

export function pendingKey(ch: number, id: string): string {
  return `${ch}:${id}`;
}

/** True for relay-usable values (non-finite numbers are never confirmed). */
export function validValue(v: unknown): v is ParamValue {
  return (
    typeof v === 'string' || typeof v === 'boolean' || (typeof v === 'number' && Number.isFinite(v))
  );
}

/**
 * Merge a `params` diff/snapshot into the confirmed map. Invalid entries are
 * skipped. Returns true when anything actually changed.
 */
export function applyParams(maps: StoreMaps, ch: number, values: Record<string, ParamValue>): boolean {
  if (!Number.isInteger(ch) || ch < 0 || values === null || typeof values !== 'object') return false;
  let slot = maps.confirmed[ch];
  let changed = false;
  for (const [id, v] of Object.entries(values)) {
    if (!id || !validValue(v)) continue;
    if (!slot) {
      slot = {};
      maps.confirmed[ch] = slot;
    }
    if (!valuesEqual(slot[id], v)) {
      slot[id] = v;
      changed = true;
    }
    confirmInto(maps.pending, pendingKey(ch, id), v);
  }
  return changed;
}

/** Numbers compare within the desk's quantization step. The engine's
 * write-path re-read agreement (engine.rs `refresh_agrees`) treats floats as
 * agreeing within ~2e-3 (~10-bit fader steps: a 0.1 write reads back
 * ~0.0997), so the confirm epsilon must cover the same quantum — otherwise a
 * confirmed echo can never clear its optimistic edit. */
export function valuesEqual(a: ParamValue | undefined, b: ParamValue): boolean {
  if (typeof a === 'number' && typeof b === 'number') return Math.abs(a - b) < 2e-3;
  return a === b;
}

/** Drop a pending edit when the relay confirms the same value. */
export function confirmInto(pending: PendingMap, key: string, value: ParamValue): boolean {
  const edit = pending[key];
  if (!edit) return false;
  if (valuesEqual(edit.value, value)) {
    delete pending[key];
    return true;
  }
  return false;
}

/** Record an optimistic edit. The display shows the pending value at once. */
export function noteOptimistic(maps: StoreMaps, ch: number, id: string, value: ParamValue, now: number): string {
  const key = pendingKey(ch, id);
  maps.pending[key] = { value, at: now };
  return key;
}

/** Drop pending edits older than `timeoutMs`; returns the expired keys. */
export function sweepPending(pending: PendingMap, now: number, timeoutMs: number): string[] {
  const expired: string[] = [];
  for (const [key, edit] of Object.entries(pending)) {
    if (now - edit.at >= timeoutMs) {
      delete pending[key];
      expired.push(key);
    }
  }
  return expired;
}

/** Split a pending key back into channel + id (null when malformed). */
export function splitKey(key: string): { ch: number; id: string } | null {
  const i = key.indexOf(':');
  if (i <= 0) return null;
  const ch = Number(key.slice(0, i));
  const id = key.slice(i + 1);
  if (!Number.isInteger(ch) || ch < 0 || !id) return null;
  return { ch, id };
}

/** Display value: pending wins, else confirmed, else null. */
export function optimisticValue(maps: StoreMaps, ch: number, id: string): ParamValue | null {
  const edit = maps.pending[pendingKey(ch, id)];
  if (edit) return edit.value;
  const v = maps.confirmed[ch]?.[id];
  return v === undefined ? null : v;
}

/** A confirmed value arrived (pending alone does not count). */
export function confirmedKnown(maps: StoreMaps, ch: number, id: string): boolean {
  return maps.confirmed[ch]?.[id] !== undefined;
}

/** True while an optimistic edit awaits confirmation. */
export function isPending(maps: StoreMaps, ch: number, id: string): boolean {
  return maps.pending[pendingKey(ch, id)] !== undefined;
}

/**
 * Validate + clamp a UI write against the caps row. Null when the write must
 * not be sent (unsupported id, wrong kind). Floats clamp into range; enums
 * round to a whole number in range; bools/text pass through.
 */
export function coerceWrite(def: CapsRow | undefined, v: ParamValue): ParamValue | null {
  if (!def) return null;
  if (def.kind === 'bool') return typeof v === 'boolean' ? v : null;
  if (def.kind === 'text') return typeof v === 'string' ? v : null;
  if (typeof v !== 'number' || !Number.isFinite(v)) return null;
  if (def.kind === 'enum') {
    const r = Math.round(v);
    return r >= def.min && r <= def.max ? r : null;
  }
  return Math.min(def.max, Math.max(def.min, v));
}

/** Normalize a focus list: finite ints >= 0, deduped, order kept. */
export function normalizeFocus(chs: readonly unknown[]): number[] {
  const out: number[] = [];
  const seen = new Set<number>();
  for (const c of chs) {
    if (typeof c !== 'number' || !Number.isInteger(c) || c < 0 || seen.has(c)) continue;
    seen.add(c);
    out.push(c);
  }
  return out;
}

export function sameList(a: readonly number[], b: readonly number[]): boolean {
  return a.length === b.length && a.every((v, i) => v === b[i]);
}

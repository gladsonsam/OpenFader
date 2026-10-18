// L0 profile migration: versioned chain vN -> vN+1. Never throws.
// v1 -> v2 example included: v1 used { op:'setProp', id, key, value } and a
// separate `hidden: string[]` list; v2 uses { op:'set', id, path, value }
// with `props.`-rooted paths and { op:'hide', id, hidden } ops.

import { PROFILE_V } from './io.ts';

export type MigrateResult = { ok: true; profile: unknown } | { ok: false; errors: string[] };

function isRecord(v: unknown): v is Record<string, unknown> {
  return typeof v === 'object' && v !== null && !Array.isArray(v);
}

type Migration = (doc: Record<string, unknown>) => Record<string, unknown>;

/** v1 -> v2: setProp ops become set ops; hidden[] list becomes hide ops. */
function migrateV1toV2(doc: Record<string, unknown>): Record<string, unknown> {
  const ops: unknown[] = Array.isArray(doc['ops']) ? [...(doc['ops'] as unknown[])] : [];
  const out: unknown[] = [];
  for (const o of ops) {
    if (isRecord(o) && o['op'] === 'setProp' && typeof o['id'] === 'string' && typeof o['key'] === 'string') {
      out.push({ op: 'set', id: o['id'], path: `props.${o['key'] as string}`, value: o['value'] });
    } else {
      out.push(o);
    }
  }
  const hidden = doc['hidden'];
  if (Array.isArray(hidden)) {
    for (const id of hidden) {
      if (typeof id === 'string') out.push({ op: 'hide', id, hidden: true });
    }
  }
  const { hidden: _drop, ...rest } = doc;
  void _drop;
  return { ...rest, v: 2, ops: out };
}

const MIGRATIONS: Record<number, Migration> = {
  1: migrateV1toV2,
};

/** Migrate any parsed document to the current version. Never throws. */
export function migrateProfile(input: unknown): MigrateResult {
  try {
    if (!isRecord(input)) return { ok: false, errors: ['profile must be an object'] };
    let doc: Record<string, unknown> = { ...(input as Record<string, unknown>) };
    let v = doc['v'];
    if (v === undefined) {
      doc['v'] = 1;
      v = 1;
    }
    if (typeof v !== 'number' || !Number.isInteger(v) || v < 1) {
      return { ok: false, errors: [`unsupported profile version: ${String(v)}`] };
    }
    if (v > PROFILE_V) return { ok: false, errors: [`profile version ${v} is newer than supported ${PROFILE_V}`] };
    let cur = v;
    while (cur < PROFILE_V) {
      const step = MIGRATIONS[cur];
      if (!step) return { ok: false, errors: [`no migration from v${cur}`] };
      doc = step(doc);
      cur += 1;
      doc['v'] = cur;
    }
    return { ok: true, profile: doc };
  } catch (err) {
    return { ok: false, errors: [err instanceof Error ? err.message : 'migration failed'] };
  }
}

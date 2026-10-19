// L0 profile migration: versioned chain vN -> vN+1. Never throws.
// v1 -> v2 example included: v1 used { op:'setProp', id, key, value } and a
// separate `hidden: string[]` list; v2 uses { op:'set', id, path, value }
// with `props.`-rooted paths and { op:'hide', id, hidden } ops.
// v2 -> v3: profile banks store stable strip keys (`ch/01`, `bus/01`, …,
// `main`) instead of raw positional indices (which mean different strips
// on sim/xair/dl32s). Numeric items map through the sim topology (the
// dev default all v2 banks were authored against); existing key strings
// pass through (with the `mix/..` alias normalised to `bus/..`).

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
  2: migrateV2toV3,
};

/** Sim topology (dev default every v2 bank was authored against): ordered
 * families in the global strip index space. Mirrors
 * `crates/sim/src/lib.rs` (`SIM_TOPOLOGY`) with the canonical `bus`
 * spelling for the sim `mix` family (see `stripKeyForSimIndex`). */
const SIM_FAMILIES: Array<{ key: string; first: number; count: number }> = [
  { key: 'ch', first: 0, count: 32 },
  { key: 'bus', first: 32, count: 8 },
  { key: 'ch2', first: 40, count: 16 },
  { key: 'fxret', first: 56, count: 4 },
  { key: 'fxsnd', first: 60, count: 4 },
  { key: 'mtx', first: 64, count: 8 },
  { key: 'main', first: 72, count: 1 },
  { key: 'dca', first: 73, count: 4 },
];

/** Stable strip key for a sim global strip index (see
 * `strip_key_for_index` in `crates/core/src/lib.rs`): `{family}/{nn}`
 * with the 1-based family index padded to 2 digits, bare `main` for the
 * single-strip family. `null` when the index names no strip. */
export function stripKeyForSimIndex(index: number): string | null {
  if (!Number.isInteger(index) || index < 0) return null;
  for (const f of SIM_FAMILIES) {
    if (index >= f.first && index < f.first + f.count) {
      if (f.count === 1) return f.key;
      return `${f.key}/${String(index - f.first + 1).padStart(2, '0')}`;
    }
  }
  return null;
}

/** Parse a stable strip key into `[family, 1-based index]` (`main` reads
 * as `['main', 1]`; the legacy `mix/..` alias normalises to `bus`).
 * Returns `null` for malformed keys. Mirrors `parse_strip_key` in
 * `crates/core/src/lib.rs`. */
export function parseStripKey(key: string): [string, number] | null {
  const t = key.trim();
  if (!t) return null;
  if (t === 'main') return ['main', 1];
  const slash = t.indexOf('/');
  if (slash < 0 || t.indexOf('/', slash + 1) >= 0) return null;
  let fam = t.slice(0, slash).trim();
  const num = t.slice(slash + 1).trim();
  if (fam === 'mix') fam = 'bus';
  if (!fam || !num || /[\s/]/.test(fam)) return null;
  if (!/^\d+$/.test(num)) return null;
  const idx = parseInt(num, 10);
  if (!Number.isSafeInteger(idx) || idx < 1) return null;
  if (fam === 'main') return idx === 1 ? ['main', 1] : null;
  return [fam, idx];
}

/** Normalise one v2 bank item to its v3 stable-key form. Numbers and
 * numeric strings map through the sim topology; existing key strings pass
 * through canonicalised (`mix/..` -> `bus/..`, `main/1` -> `main`,
 * single digits padded). Anything else (fixture ids, garbage) passes
 * through untouched — migration never drops a bank entry. */
export function normalizeBankItem(item: unknown): string {
  if (typeof item === 'number' && Number.isInteger(item)) {
    return stripKeyForSimIndex(item) ?? String(item);
  }
  if (typeof item !== 'string') return String(item);
  const t = item.trim();
  if (/^\d+$/.test(t)) {
    return stripKeyForSimIndex(parseInt(t, 10)) ?? t;
  }
  const parsed = parseStripKey(t);
  if (!parsed) return item;
  const [fam, idx] = parsed;
  if (fam === 'main') return 'main';
  return `${fam}/${String(idx).padStart(2, '0')}`;
}

/** v2 -> v3: bank items become stable strip keys (see above). */
function migrateV2toV3(doc: Record<string, unknown>): Record<string, unknown> {
  const banks = Array.isArray(doc['banks']) ? doc['banks'] : [];
  const out = banks.map((b) => {
    if (typeof b !== 'object' || b === null || Array.isArray(b)) return b;
    const rec = b as Record<string, unknown>;
    if (!Array.isArray(rec['items'])) return rec;
    return { ...rec, items: (rec['items'] as unknown[]).map(normalizeBankItem) };
  });
  return { ...doc, v: 3, banks: out };
}

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

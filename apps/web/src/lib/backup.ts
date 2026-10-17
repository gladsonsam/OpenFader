// Whole-app settings backup: one versioned JSON file holding every
// localStorage entry under `openmix.*` (layers, layer order, custom tabs,
// prefs, shows cache + active show, sends, solo/mute groups, onboarding).
//
// Envelope:
//
//   { kind: "openmix-backup", version: 1, app, exportedAt, entries: { key: rawString } }
//
// Values stay raw strings (already-serialized JSON or plain flags) so the
// backup never needs to understand each pref's shape — restore writes them
// back verbatim. `parseBackupFileText` never throws: malformed input becomes
// `{ ok: false, error }` for the UI, mirroring lib/showFile.ts.
//
// Import safety: validate-all-then-write. Unknown keys (not under
// `openmix.`) are kept aside with a warning, never applied; corrupt files
// are rejected before anything is written.
//
// Pure + DOM-free: runs in the browser and under plain Node (roundtrip +
// fuzz-ish test in backup.test.ts via `node --experimental-strip-types`).

export const BACKUP_KIND = 'openmix-backup';
export const BACKUP_VERSION = 1;
export const BACKUP_FILE_NAME = 'openmix-backup-v1.json';

/** Storage key prefix covered by the backup (everything else is ignored). */
export const BACKUP_PREFIX = 'openmix.';

/** Max accepted file text (8 MiB — settings are tens of KB; beyond is junk). */
export const BACKUP_MAX_CHARS = 8 * 1024 * 1024;

/** Max single value length (4 MiB — one pref bigger than this is junk). */
export const BACKUP_MAX_VALUE_CHARS = 4 * 1024 * 1024;

/** Max entry count (settings keys are ~dozens; beyond is junk). */
export const BACKUP_MAX_ENTRIES = 256;

export interface BackupFile {
  kind: typeof BACKUP_KIND;
  version: typeof BACKUP_VERSION;
  /** Exporting app version (cf. apps/web/package.json). */
  app: string;
  /** ISO-8601 export timestamp. */
  exportedAt: string;
  /** Raw localStorage values keyed by full key (verbatim roundtrip). */
  entries: Record<string, string>;
  /** Keys present in the file but outside `openmix.*`: kept aside, never applied. */
  unknownKeys: string[];
}

export interface BackupPreviewEntry {
  key: string;
  bytes: number;
  known: boolean;
}

export interface BackupPreview {
  keys: number;
  bytes: number;
  entries: BackupPreviewEntry[];
  unknownKeys: string[];
}

export type BackupParseResult =
  | { ok: true; file: BackupFile; warnings: string[] }
  | { ok: false; error: string };

function fail(error: string): BackupParseResult {
  return { ok: false, error };
}

function isRec(v: unknown): v is Record<string, unknown> {
  return v !== null && typeof v === 'object' && !Array.isArray(v);
}

function byteLen(s: string): number {
  try {
    return new TextEncoder().encode(s).length;
  } catch {
    return s.length;
  }
}

/**
 * Filter a full key->value snapshot down to backup scope (`openmix.*`).
 * Pure (takes a plain record so it stays DOM-free and Node-testable).
 */
export function collectBackupEntries(all: Record<string, string>): Record<string, string> {
  const out: Record<string, string> = {};
  for (const k of Object.keys(all)) {
    if (k.startsWith(BACKUP_PREFIX) && typeof all[k] === 'string') out[k] = all[k] as string;
  }
  return out;
}

/** Read every localStorage entry (DOM; thin wrapper over collectBackupEntries). */
export function readLiveEntries(): Record<string, string> {
  const all: Record<string, string> = {};
  try {
    for (let i = 0; i < localStorage.length; i++) {
      const k = localStorage.key(i);
      if (k === null) continue;
      const v = localStorage.getItem(k);
      if (typeof v === 'string') all[k] = v;
    }
  } catch {
    /* storage blocked: backup exports whatever was readable (maybe nothing) */
  }
  return collectBackupEntries(all);
}

/** Build an export envelope (copied, so later edits can't alias it). */
export function buildBackupFile(
  entries: Record<string, string>,
  opts: { app: string; exportedAt?: string },
): BackupFile {
  const known: Record<string, string> = {};
  const unknownKeys: string[] = [];
  for (const k of Object.keys(entries)) {
    if (k.startsWith(BACKUP_PREFIX)) known[k] = entries[k] as string;
    else if (!unknownKeys.includes(k)) unknownKeys.push(k);
  }
  unknownKeys.sort();
  return {
    kind: BACKUP_KIND,
    version: BACKUP_VERSION,
    app: opts.app,
    exportedAt: opts.exportedAt ?? new Date().toISOString(),
    entries: { ...known },
    unknownKeys,
  };
}

export function serializeBackupFile(file: BackupFile): string {
  return JSON.stringify(
    {
      kind: file.kind,
      version: file.version,
      app: file.app,
      exportedAt: file.exportedAt,
      entries: file.entries,
    },
    null,
    2,
  );
}

/**
 * Validate + normalize a backup file text. Never throws: garbage in ->
 * `{ ok: false, error }` with a message safe to show the user.
 * Unknown (non-`openmix.*`) keys are kept aside in `file.unknownKeys`
 * with a warning and are never written by `restoreBackupEntries`.
 */
export function parseBackupFileText(text: string): BackupParseResult {
  if (typeof text !== 'string' || !text.trim()) return fail('empty file: not a settings backup');
  if (text.length > BACKUP_MAX_CHARS) return fail('file too large: not a settings backup');
  let raw: unknown;
  try {
    raw = JSON.parse(text);
  } catch {
    return fail('unreadable file: not JSON (truncated or corrupt?)');
  }
  if (!isRec(raw)) return fail('not a settings backup: top level must be an object');
  if (raw['kind'] !== BACKUP_KIND) {
    const k = typeof raw['kind'] === 'string' ? `"${raw['kind']}"` : typeof raw['kind'];
    return fail(`not an openmix settings backup (kind=${k})`);
  }
  if (raw['version'] !== BACKUP_VERSION) {
    return fail(
      `unsupported backup version ${String(raw['version'])} (this app reads v${BACKUP_VERSION})`,
    );
  }
  if (!isRec(raw['entries'])) return fail('bad settings backup: entries is not an object');
  const rawEntries = raw['entries'] as Record<string, unknown>;
  const keys = Object.keys(rawEntries);
  if (keys.length > BACKUP_MAX_ENTRIES) return fail('bad settings backup: too many entries');
  const entries: Record<string, string> = {};
  const unknownKeys: string[] = [];
  for (const k of keys) {
    const v = rawEntries[k];
    if (typeof v !== 'string') return fail(`bad settings backup: value for "${k}" is not a string`);
    if (v.length > BACKUP_MAX_VALUE_CHARS)
      return fail(`bad settings backup: value for "${k}" is too large`);
    if (typeof k !== 'string' || !k) return fail('bad settings backup: entry key is empty');
    if (!k.startsWith(BACKUP_PREFIX)) {
      if (!unknownKeys.includes(k)) unknownKeys.push(k);
      continue; // kept aside: validated, never applied
    }
    entries[k] = v;
  }
  unknownKeys.sort();
  const warnings: string[] = unknownKeys.length
    ? [`${unknownKeys.length} unknown key${unknownKeys.length === 1 ? '' : 's'} kept aside (not restored): ${unknownKeys.join(', ')}`]
    : [];
  const file: BackupFile = {
    kind: BACKUP_KIND,
    version: BACKUP_VERSION,
    app: typeof raw['app'] === 'string' ? raw['app'] : 'unknown',
    exportedAt: typeof raw['exportedAt'] === 'string' ? raw['exportedAt'] : 'unknown',
    entries,
    unknownKeys,
  };
  return { ok: true, file, warnings };
}

/** Key list + byte sizes for the pre-restore import preview. */
export function previewBackup(file: BackupFile): BackupPreview {
  const keys = Object.keys(file.entries).sort();
  const entries: BackupPreviewEntry[] = keys.map((key) => ({
    key,
    bytes: byteLen(file.entries[key] as string),
    known: true,
  }));
  for (const key of file.unknownKeys) {
    if (!entries.some((e) => e.key === key)) entries.push({ key, bytes: 0, known: false });
  }
  entries.sort((a, b) => (a.key < b.key ? -1 : a.key > b.key ? 1 : 0));
  let bytes = 0;
  for (const e of entries) bytes += e.bytes;
  return { keys: keys.length, bytes, entries, unknownKeys: [...file.unknownKeys] };
}

/**
 * Write validated entries into a plain record target (pure, Node-testable).
 * Validate-all-then-write: throws nothing; a non-string value aborts the
 * whole apply and the target is left untouched.
 */
export function restoreBackupEntries(
  file: BackupFile,
  target: Record<string, string>,
): { applied: string[]; skippedUnknown: string[] } {
  const keys = Object.keys(file.entries).sort();
  for (const k of keys) {
    const v = file.entries[k];
    if (typeof v !== 'string' || !k.startsWith(BACKUP_PREFIX)) {
      return { applied: [], skippedUnknown: [...file.unknownKeys] };
    }
    if ((v as string).length > BACKUP_MAX_VALUE_CHARS) {
      return { applied: [], skippedUnknown: [...file.unknownKeys] };
    }
  }
  const applied: string[] = [];
  for (const k of keys) {
    target[k] = file.entries[k] as string;
    applied.push(k);
  }
  return { applied, skippedUnknown: [...file.unknownKeys] };
}

/**
 * Restore validated entries to live localStorage (DOM).
 * Nothing is written unless every entry validates.
 */
export function restoreToLocalStorage(file: BackupFile): { applied: string[]; skippedUnknown: string[] } {
  const scratch: Record<string, string> = {};
  const check = restoreBackupEntries(file, scratch);
  if (!check.applied.length && Object.keys(file.entries).length) return check;
  const applied: string[] = [];
  try {
    for (const k of check.applied) {
      localStorage.setItem(k, scratch[k] as string);
      applied.push(k);
    }
  } catch {
    /* storage blocked mid-write: report what landed; caller surfaces it */
  }
  return { applied, skippedUnknown: check.skippedUnknown };
}

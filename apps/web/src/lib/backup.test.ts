// Roundtrip + fuzz-ish validation test for backup.ts (no deps, no DOM).
// Run: node --experimental-strip-types src/lib/backup.test.ts
// (must exit silently with code 0; any failure throws).
import {
  BACKUP_VERSION,
  buildBackupFile,
  collectBackupEntries,
  parseBackupFileText,
  previewBackup,
  restoreBackupEntries,
  serializeBackupFile,
} from './backup.ts';

function eq(a: unknown, b: unknown, msg: string): void {
  if (JSON.stringify(a) !== JSON.stringify(b)) throw new Error(`FAIL ${msg}`);
}

// 1. collect keeps only openmix.* keys.
eq(
  collectBackupEntries({ 'openmix.layers.v1': '[]', 'other': 'x', 'openmixx': 'y' }),
  { 'openmix.layers.v1': '[]' },
  'collect prefix filter',
);

// 2. export -> serialize -> parse -> equal (roundtrip).
const entries: Record<string, string> = {
  'openmix.layers.v1': JSON.stringify([{ id: 'inputs', name: 'Inputs' }]),
  'openmix.shows.v1': JSON.stringify([{ name: 'Gig', scenes: [] }]),
  'openmix.onboarded.v1': '1',
};
const file = buildBackupFile(entries, { app: '0.1.0-test' });
if (file.kind !== 'openmix-backup') throw new Error('FAIL kind');
if (file.version !== BACKUP_VERSION) throw new Error('FAIL version');
if (!file.exportedAt) throw new Error('FAIL exportedAt');
const parsed = parseBackupFileText(serializeBackupFile(file));
if (!parsed.ok) throw new Error(`FAIL roundtrip parse: ${parsed.error}`);
eq(parsed.file.entries, entries, 'roundtrip entries equal');
if (parsed.warnings.length) throw new Error('FAIL unexpected warnings');
if (parsed.file.unknownKeys.length) throw new Error('FAIL unexpected unknown');

// 3. preview lists keys + sizes.
const pv = previewBackup(parsed.file);
if (pv.keys !== 3) throw new Error('FAIL preview key count');
if (pv.bytes <= 0) throw new Error('FAIL preview bytes');
eq(
  pv.entries.map((e) => e.key),
  ['openmix.layers.v1', 'openmix.onboarded.v1', 'openmix.shows.v1'],
  'preview sorted keys',
);
for (const e of pv.entries) {
  if (!e.known || e.bytes <= 0) throw new Error(`FAIL preview entry ${e.key}`);
}

// 4. restore writes all-or-nothing into a plain record (validate-all-then-write).
const target: Record<string, string> = { 'openmix.layers.v1': 'old' };
const res = restoreBackupEntries(parsed.file, target);
eq(res.applied.length, 3, 'restore applied all');
eq(Object.keys(target).sort(), Object.keys(entries).sort(), 'restore target keys');
for (const k of Object.keys(entries)) eq(target[k], entries[k], `restore value ${k}`);
eq(res.skippedUnknown, [], 'restore no unknown skipped');

// 4b. corrupt entry aborts before touching the target (never half-applied).
const badFile = { ...parsed.file, entries: { ...entries, 'openmix.layers.v1': 42 as unknown as string } };
const target2: Record<string, string> = { 'openmix.layers.v1': 'old' };
const res2 = restoreBackupEntries(badFile, target2);
eq(res2.applied, [], 'corrupt restore applies nothing');
eq(target2, { 'openmix.layers.v1': 'old' }, 'corrupt restore target untouched');

// 5. unknown keys kept aside with warning, never applied.
const withUnknown = parseBackupFileText(
  JSON.stringify({
    kind: 'openmix-backup',
    version: 1,
    app: 't',
    exportedAt: 't',
    entries: { 'openmix.fine.v1': 'true', 'evil.key': 'x' },
  }),
);
if (!withUnknown.ok) throw new Error(`FAIL unknown rejected: ${withUnknown.error}`);
eq(withUnknown.file.unknownKeys, ['evil.key'], 'unknown kept aside');
if (!withUnknown.warnings.length) throw new Error('FAIL missing unknown warning');
const target3: Record<string, string> = {};
const res3 = restoreBackupEntries(withUnknown.file, target3);
eq(target3, { 'openmix.fine.v1': 'true' }, 'unknown never applied');
eq(res3.skippedUnknown, ['evil.key'], 'unknown reported skipped');

// 6. fuzz-ish: garbage in -> clean { ok: false }, never throws.
const bad: unknown[] = [
  '',
  '   ',
  '{',
  '{"kind":"openmix-backup"',
  'null',
  '[]',
  '{}',
  '{"kind":"nope","version":1,"entries":{}}',
  '{"kind":"openmix-backup","version":999,"entries":{}}',
  '{"kind":"openmix-backup","version":1}',
  '{"kind":"openmix-backup","version":1,"entries":[]}',
  '{"kind":"openmix-backup","version":1,"entries":{"openmix.a":42}}',
  '{"kind":"openmix-backup","version":1,"entries":{"":"x"}}',
  '[1,2,3]',
  '"openmix-backup"',
  '42',
];
for (const b of bad) {
  let r;
  try {
    r = parseBackupFileText(String(b));
  } catch {
    throw new Error(`FAIL threw on ${JSON.stringify(String(b)).slice(0, 40)}`);
  }
  if (r.ok) throw new Error(`FAIL accepted ${JSON.stringify(String(b)).slice(0, 60)}`);
  if (!r.error) throw new Error('FAIL empty error message');
}

// 7. truncation at every 16th byte still rejects cleanly, never throws.
const good = serializeBackupFile(file);
for (let n = 0; n < good.length; n += 16) {
  const r = parseBackupFileText(good.slice(0, n));
  if (r.ok) throw new Error(`FAIL truncated@${n} accepted`);
}

console.log('backup.test OK');

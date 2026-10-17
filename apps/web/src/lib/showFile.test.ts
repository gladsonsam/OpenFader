// Roundtrip + fuzz-ish validation test for showFile.ts (no deps, no DOM).
// Run: node --experimental-strip-types src/lib/showFile.test.ts
// (must exit silently with code 0; any failure throws).
import {
  buildShowFile,
  mergeImported,
  parseShowFileText,
  previewShowFile,
  serializeShowFile,
  uniqueShowName,
  SHOW_FILE_VERSION,
} from './showFile.ts';
import type { ShowLib } from './backend.ts';

function eq(a: unknown, b: unknown, msg: string): void {
  if (JSON.stringify(a) !== JSON.stringify(b)) throw new Error(`FAIL ${msg}`);
}

const lib: ShowLib[] = [
  {
    name: 'Gig',
    scenes: [
      {
        name: 'verse',
        strips: [
          { ch: 0, name: 'Vox', fader: 0.8, muted: false },
          { ch: 1, name: 'Gtr', fader: 0.6, muted: true },
        ],
        main_fader: 0.9,
      },
      { name: 'chorus', strips: [], main_fader: null },
    ],
  },
];

// 1. export -> serialize -> parse -> equal (roundtrip).
const file = buildShowFile(lib, { app: '0.1.0-test', driver: 'ws' });
if (file.kind !== 'openmix-show') throw new Error('FAIL kind');
if (file.version !== SHOW_FILE_VERSION) throw new Error('FAIL version');
if (!file.exportedAt) throw new Error('FAIL exportedAt');
if (file.driver !== 'ws') throw new Error('FAIL driver');
const parsed = parseShowFileText(serializeShowFile(file));
if (!parsed.ok) throw new Error(`FAIL roundtrip parse: ${parsed.error}`);
eq(parsed.file.shows, lib, 'roundtrip shows equal');

// 2. preview counts.
const pv = previewShowFile(parsed.file);
eq(pv, { shows: 1, scenes: 2, strips: 2, names: ['Gig'] }, 'preview counts');

// 3. merge never overwrites: same name twice -> suffixed newcomers.
const m1 = mergeImported(lib, parsed.file);
eq(m1.added, ['Gig (imported)'], 'merge suffix');
eq(m1.merged.length, 2, 'merge appends');
eq(m1.merged[0], lib[0], 'merge keeps existing');
const m2 = mergeImported(m1.merged, parsed.file);
eq(m2.added, ['Gig (imported 2)'], 'merge second suffix');
if (uniqueShowName('  ', []) !== 'Imported (imported)') throw new Error('FAIL blank base');

// 4. fuzz-ish: garbage in -> clean { ok: false } , never throws.
const bad: unknown[] = [
  '',
  '   ',
  '{',
  '{"kind":"openmix-show"',
  'null',
  '[]',
  '{}',
  '{"kind":"nope","version":1,"shows":[]}',
  '{"kind":"openmix-show","version":999,"shows":[]}',
  '{"kind":"openmix-show","version":1,"shows":[]}',
  '{"kind":"openmix-show","version":1,"shows":[{}]}',
  '{"kind":"openmix-show","version":1,"shows":[{"name":"","scenes":[]}]}',
  '{"kind":"openmix-show","version":1,"shows":[{"name":"x","scenes":{}}]}',
  '{"kind":"openmix-show","version":1,"shows":[{"name":"x","scenes":[{"name":"y"}]}]}',
  '{"kind":"openmix-show","version":1,"shows":[{"name":"x","scenes":[{"name":"y","strips":[{"ch":"0"}]}]}]}',
  '[1,2,3]',
  '"openmix-show"',
  '42',
];
for (const b of bad) {
  let r;
  try {
    r = parseShowFileText(String(b));
  } catch {
    throw new Error(`FAIL threw on ${JSON.stringify(String(b)).slice(0, 40)}`);
  }
  if (r.ok) throw new Error(`FAIL accepted ${JSON.stringify(String(b)).slice(0, 60)}`);
  if (!r.error) throw new Error('FAIL empty error message');
}

// 5. truncation at every 16th byte still rejects cleanly, never throws.
const good = serializeShowFile(file);
for (let n = 0; n < good.length; n += 16) {
  const r = parseShowFileText(good.slice(0, n));
  if (r.ok) throw new Error(`FAIL truncated@${n} accepted`);
}

console.log('showFile.test OK');

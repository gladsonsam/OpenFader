// Shareable show files (Mixing Station community-sharing parity, offline-first).
//
// Versioned JSON envelope around one or more ShowLib shows:
//
//   { kind: "openmix-show", version: 1, app, exportedAt, driver?, shows: [...] }
//
// `parseShowFileText` never throws: every malformed input (truncated JSON,
// wrong shapes, unknown versions) becomes a `{ ok: false, error }` with a
// human-readable message, so the import UI can show it instead of crashing.
//
// Pure + DOM-free: runs in the browser and under plain Node (roundtrip +
// fuzz-ish test in showFile.test.ts via `node --experimental-strip-types`).
// Show/Scene/strip shapes mirror lib/backend.ts (cf. core Show/Scene).

import type { ShowLib, ShowScene, ShowSceneStrip } from './backend';

export const SHOW_FILE_KIND = 'openmix-show';
export const SHOW_FILE_VERSION = 1;
export const SHOW_FILE_NAME = 'openmix-show-v1.json';
export const SHOW_LIB_FILE_NAME = 'openmix-library-v1.json';

/** Max accepted file text (8 MiB — a library is tens of KB; beyond is junk). */
export const SHOW_FILE_MAX_CHARS = 8 * 1024 * 1024;

export interface ShowFile {
  kind: typeof SHOW_FILE_KIND;
  version: typeof SHOW_FILE_VERSION;
  /** Exporting app version (cf. apps/web/package.json). */
  app: string;
  /** ISO-8601 export timestamp. */
  exportedAt: string;
  /** Transport hint at export time (e.g. "ws" | "tauri"). */
  driver?: string;
  shows: ShowLib[];
}

export interface ShowFilePreview {
  shows: number;
  scenes: number;
  strips: number;
  names: string[];
}

export type ParseResult = { ok: true; file: ShowFile } | { ok: false; error: string };

function fail(error: string): ParseResult {
  return { ok: false, error };
}

function isRec(v: unknown): v is Record<string, unknown> {
  return v !== null && typeof v === 'object' && !Array.isArray(v);
}

function copyStrip(s: ShowSceneStrip): ShowSceneStrip {
  return { ch: s.ch, name: s.name, fader: s.fader, muted: s.muted };
}

function copyShows(shows: ShowLib[]): ShowLib[] {
  return shows.map((s) => ({
    name: s.name,
    scenes: s.scenes.map((sc) => ({
      name: sc.name,
      strips: sc.strips.map(copyStrip),
      main_fader: sc.main_fader,
    })),
  }));
}

/** Build an export envelope (deep-copied, so later edits can't alias it). */
export function buildShowFile(
  shows: ShowLib[],
  opts: { app: string; driver?: string; exportedAt?: string },
): ShowFile {
  const file: ShowFile = {
    kind: SHOW_FILE_KIND,
    version: SHOW_FILE_VERSION,
    app: opts.app,
    exportedAt: opts.exportedAt ?? new Date().toISOString(),
    shows: copyShows(shows),
  };
  if (opts.driver) file.driver = opts.driver;
  return file;
}

export function serializeShowFile(file: ShowFile): string {
  return JSON.stringify(file, null, 2);
}

function parseStrip(v: unknown, where: string): ShowSceneStrip | string {
  if (!isRec(v)) return `${where}: strip is not an object`;
  const ch = v['ch'];
  if (typeof ch !== 'number' || !Number.isFinite(ch)) return `${where}: bad channel`;
  const f = Number(v['fader'] ?? 0.75);
  return {
    ch,
    name: typeof v['name'] === 'string' ? v['name'] : `CH${ch + 1}`,
    fader: Number.isFinite(f) ? Math.min(1, Math.max(0, f)) : 0.75,
    muted: !!v['muted'],
  };
}

function parseScene(v: unknown, si: number): ShowScene | string {
  if (!isRec(v)) return `scene #${si + 1}: not an object`;
  if (typeof v['name'] !== 'string' || !v['name']) return `scene #${si + 1}: missing name`;
  if (!Array.isArray(v['strips'])) return `scene "${v['name']}": strips is not an array`;
  const strips: ShowSceneStrip[] = [];
  for (let i = 0; i < v['strips'].length; i++) {
    const s = parseStrip(v['strips'][i], `scene "${v['name']}" strip #${i + 1}`);
    if (typeof s === 'string') return s;
    strips.push(s);
  }
  const mf = v['main_fader'];
  return {
    name: v['name'],
    strips,
    main_fader: typeof mf === 'number' && Number.isFinite(mf) ? Math.min(1, Math.max(0, mf)) : null,
  };
}

function parseShow(v: unknown, si: number): ShowLib | string {
  if (!isRec(v)) return `show #${si + 1}: not an object`;
  if (typeof v['name'] !== 'string' || !v['name']) return `show #${si + 1}: missing name`;
  if (!Array.isArray(v['scenes'])) return `show "${v['name']}": scenes is not an array`;
  const scenes: ShowScene[] = [];
  for (let i = 0; i < v['scenes'].length; i++) {
    const s = parseScene(v['scenes'][i], i);
    if (typeof s === 'string') return s;
    scenes.push(s);
  }
  return { name: v['name'], scenes };
}

/**
 * Validate + normalize a show-file text. Never throws: garbage in ->
 * `{ ok: false, error }` with a message safe to show the user.
 */
export function parseShowFileText(text: string): ParseResult {
  if (typeof text !== 'string' || !text.trim()) return fail('empty file: not a show export');
  if (text.length > SHOW_FILE_MAX_CHARS) return fail('file too large: not a show export');
  let raw: unknown;
  try {
    raw = JSON.parse(text);
  } catch {
    return fail('unreadable file: not JSON (truncated or corrupt?)');
  }
  if (!isRec(raw)) return fail('not a show file: top level must be an object');
  if (raw['kind'] !== SHOW_FILE_KIND) {
    const k = typeof raw['kind'] === 'string' ? `"${raw['kind']}"` : typeof raw['kind'];
    return fail(`not an openmix show file (kind=${k})`);
  }
  if (raw['version'] !== SHOW_FILE_VERSION) {
    return fail(
      `unsupported show file version ${String(raw['version'])} (this app reads v${SHOW_FILE_VERSION})`,
    );
  }
  if (!Array.isArray(raw['shows']) || !raw['shows'].length) {
    return fail('show file has no shows to import');
  }
  const shows: ShowLib[] = [];
  for (let i = 0; i < raw['shows'].length; i++) {
    const s = parseShow(raw['shows'][i], i);
    if (typeof s === 'string') return fail(`bad show file: ${s}`);
    shows.push(s);
  }
  const file: ShowFile = {
    kind: SHOW_FILE_KIND,
    version: SHOW_FILE_VERSION,
    app: typeof raw['app'] === 'string' ? raw['app'] : 'unknown',
    exportedAt: typeof raw['exportedAt'] === 'string' ? raw['exportedAt'] : 'unknown',
    shows,
  };
  if (typeof raw['driver'] === 'string' && raw['driver']) file.driver = raw['driver'];
  return { ok: true, file };
}

/** Counts for the pre-merge import preview. */
export function previewShowFile(file: ShowFile): ShowFilePreview {
  let scenes = 0;
  let strips = 0;
  const names: string[] = [];
  for (const s of file.shows) {
    names.push(s.name);
    scenes += s.scenes.length;
    for (const sc of s.scenes) strips += sc.strips.length;
  }
  return { shows: file.shows.length, scenes, strips, names };
}

/** "Name (imported)", suffixed until it collides with nothing. Never overwrites. */
export function uniqueShowName(base: string, existing: string[]): string {
  const clean = base.trim().slice(0, 32) || 'Imported';
  let name = `${clean} (imported)`;
  for (let n = 2; existing.includes(name); n++) name = `${clean} (imported ${n})`;
  return name;
}

/** Merge an import as new show(s); existing library entries are untouched. */
export function mergeImported(existing: ShowLib[], file: ShowFile): { merged: ShowLib[]; added: string[] } {
  const merged = copyShows(existing);
  const taken = merged.map((s) => s.name);
  const added: string[] = [];
  for (const s of file.shows) {
    const name = uniqueShowName(s.name, taken);
    taken.push(name);
    added.push(name);
    merged.push({ name, scenes: s.scenes.map((sc) => ({ ...sc, strips: sc.strips.map(copyStrip) })) });
  }
  return { merged, added };
}

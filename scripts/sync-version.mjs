#!/usr/bin/env node
// App version fan-out: the VERSION file at the repo root is the single
// source of truth. This script copies it into every manifest that pins the
// app version, so a release is one edit (`VERSION`) + one run.
//
//   node scripts/sync-version.mjs          # write all targets to VERSION
//   node scripts/sync-version.mjs --check  # exit 1 on any drift (CI gate)
//
// Node only, no dependencies. Line-based TOML edits touch only the version
// key inside the target section (dependency `version = "…"` pins elsewhere
// are left alone). JSON targets are rewritten canonical 2-space (which is
// already the repo style); all other fields are preserved.
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = dirname(dirname(fileURLToPath(import.meta.url)));
const CHECK = process.argv.includes('--check');

const VERSION = readFileSync(join(ROOT, 'VERSION'), 'utf8').trim();
if (!/^\d+\.\d+\.\d+(-[\w.]+)?$/.test(VERSION)) {
  console.error(`sync-version: invalid semver in VERSION: ${JSON.stringify(VERSION)}`);
  process.exit(2);
}

let dirty = false;

function report(rel, oldV, changed) {
  if (changed) {
    dirty = true;
    console.log(`${rel}: ${oldV ?? '(missing)'} -> ${VERSION}`);
  } else {
    console.log(`${rel}: already ${VERSION}`);
  }
}

function writeUnlessCheck(rel, nextText, oldV, changed) {
  report(rel, oldV, changed);
  if (changed && !CHECK) writeFileSync(join(ROOT, rel), nextText, 'utf8');
}

// First match in the section wins; other sections (incl. [patch.crates-io]
// and dependency `version` pins) are untouched.
function syncToml(rel, section) {
  const text = readFileSync(join(ROOT, rel), 'utf8');
  const lines = text.split('\n');
  let inSection = false;
  let oldV = null;
  for (let i = 0; i < lines.length; i++) {
    const header = lines[i].match(/^\s*\[(.+?)\]\s*$/);
    if (header) {
      inSection = header[1].trim() === section;
      continue;
    }
    if (!inSection) continue;
    const m = lines[i].match(/^(\s*version\s*=\s*")([^"]*)(".*)$/);
    if (m) {
      oldV = m[2];
      if (oldV !== VERSION) lines[i] = `${m[1]}${VERSION}${m[3]}`;
      break;
    }
  }
  if (oldV === null) {
    console.error(`sync-version: no version key in [${section}] of ${rel}`);
    process.exit(2);
  }
  writeUnlessCheck(rel, lines.join('\n'), oldV, oldV !== VERSION);
}

// pubspec.yaml: `version: <semver>[+<build>]`. The semver tracks VERSION;
// a trailing `+<build>` Flutter build counter is preserved as-is.
function syncPubspec(rel) {
  const text = readFileSync(join(ROOT, rel), 'utf8');
  const lines = text.split('\n');
  let oldV = null;
  let changed = false;
  for (let i = 0; i < lines.length; i++) {
    const m = lines[i].match(/^(\s*version\s*:\s*)(\d+\.\d+\.\d+(?:-[\w.]+)?)(\+\d+)?(\s*)$/);
    if (m) {
      oldV = m[2] + (m[3] ?? '');
      const next = `${m[1]}${VERSION}${m[3] ?? ''}${m[4]}`;
      changed = m[2] !== VERSION;
      if (changed) lines[i] = next;
      break;
    }
  }
  if (oldV === null) {
    console.error(`sync-version: no version key in ${rel}`);
    process.exit(2);
  }
  writeUnlessCheck(rel, lines.join('\n'), oldV, changed);
}

syncToml('Cargo.toml', 'workspace.package');
syncPubspec('app/pubspec.yaml');

if (CHECK && dirty) {
  console.error('sync-version: drift detected (run without --check to fix)');
  process.exit(1);
}

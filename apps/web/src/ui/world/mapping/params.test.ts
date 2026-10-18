// W2a mapping tests: round trips both directions across the whole range for
// every mapped id, incl. clamping and enum maps.
// Run: node --experimental-strip-types mapping/params.test.ts

import { XAIR_LAW } from './faderLaw.ts';
import {
  COLOR_HEX,
  bankPalette,
  colorToHex,
  design01ToSendDb,
  designEqTypeToEngine,
  designPanToEngine,
  designSlopeToEngine,
  engineEqTypeToDesign,
  enginePanToDesign,
  engineSlopeToDesign,
  preToTap,
  rangeMismatches,
  rangeReport,
  sendDbToDesign01,
  tapToPre,
} from './params.ts';

let passed = 0;
let failed = 0;
const failures: string[] = [];
function ok(cond: boolean, name: string, extra?: string): void {
  if (cond) passed++;
  else {
    failed++;
    failures.push(name + (extra ? ' :: ' + extra : ''));
  }
}
function approx(a: number, b: number, eps = 1e-6): boolean {
  return Math.abs(a - b) <= eps;
}

// ---- EQ type enum 0..5 <-> LS/PEQ/HS ----

ok(engineEqTypeToDesign(0) === 'LS' && engineEqTypeToDesign(1) === 'LS', 'cut+shelf low -> LS');
ok(engineEqTypeToDesign(2) === 'PEQ' && engineEqTypeToDesign(3) === 'PEQ', 'PEQ/VEQ -> PEQ');
ok(engineEqTypeToDesign(4) === 'HS' && engineEqTypeToDesign(5) === 'HS', 'high shelf+cut -> HS');
for (let t = 0; t <= 5; t++) ok(['LS', 'PEQ', 'HS'].includes(engineEqTypeToDesign(t)), `eq type ${t} total`);
ok(designEqTypeToEngine('LS') === 1 && designEqTypeToEngine('PEQ') === 2 && designEqTypeToEngine('HS') === 4, 'design -> shelf enums (never cuts)');
// Shelf round trips are exact; cuts fold lossily (documented).
for (const t of ['LS', 'PEQ', 'HS'] as const) {
  ok(engineEqTypeToDesign(designEqTypeToEngine(t)) === t, `shelf round trip ${t}`);
}

// ---- HPF slope ----

ok(engineSlopeToDesign(0) === 12 && engineSlopeToDesign(1) === 12 && engineSlopeToDesign(2) === 24, 'slope folds 18->12');
ok(designSlopeToEngine(12) === 0 && designSlopeToEngine(24) === 2, 'slope write');
ok(engineSlopeToDesign(designSlopeToEngine(12)) === 12 && engineSlopeToDesign(designSlopeToEngine(24)) === 24, 'slope round trip');

// ---- pan: exact across the whole range ----

for (let p = -100; p <= 100; p += 5) {
  ok(approx(designPanToEngine(enginePanToDesign(p)), p, 1e-9), `pan engine round trip ${p}`);
}
for (let p = -100; p <= 100; p++) {
  const d = enginePanToDesign(p);
  ok(d >= -1 && d <= 1, `pan in design range ${p}`);
}
ok(enginePanToDesign(-100) === -1 && enginePanToDesign(100) === 1 && enginePanToDesign(0) === 0, 'pan endpoints');
ok(designPanToEngine(-2) === -100 && designPanToEngine(2) === 100, 'pan clamps');
ok(enginePanToDesign(-150) === -1 && enginePanToDesign(150) === 1, 'pan engine clamps');

// ---- sends dB <-> 0..1 via the fader law ----

for (let db = -90; db <= 10; db += 2.5) {
  const n = sendDbToDesign01(db, XAIR_LAW);
  ok(n >= 0 && n <= 1, `send01 in range db=${db}`);
  if (db <= -90) {
    ok(n === 0, 'send floor -> 0');
  } else {
    ok(approx(design01ToSendDb(n, XAIR_LAW), db, 1e-6), `send dB round trip db=${db}`);
  }
}
for (let i = 0; i <= 40; i++) {
  const v = i / 40;
  const db = design01ToSendDb(v, XAIR_LAW);
  if (v <= 0) ok(db === -90, 'send 0 -> -90 dB');
  else ok(approx(sendDbToDesign01(db, XAIR_LAW), v, 1e-6), `send 01 round trip v=${v}`);
}
ok(sendDbToDesign01(-Infinity) === 0 && design01ToSendDb(NaN) === -90, 'send non-finite guards');
ok(sendDbToDesign01(50) === 1 && design01ToSendDb(2) === 10, 'send clamps');

// ---- tap <-> pre ----

ok(tapToPre(0) === true && tapToPre(1) === false && tapToPre(0.4) === true, 'tap reads');
ok(preToTap(true) === 0 && preToTap(false) === 1, 'tap writes');
ok(tapToPre(preToTap(true)) === true && tapToPre(preToTap(false)) === false, 'tap round trip');

// ---- range report: exact rows + known mismatches ----

{
  const rows = rangeReport();
  ok(rows.length === 14, 'report covers 14 knobs', String(rows.length));
  const byId: Record<string, (typeof rows)[number]> = {};
  for (const r of rows) byId[r.engineId] = r;
  for (const exact of ['trim', 'gate_thr', 'comp_thr', 'comp_makeup']) {
    ok(byId[exact]?.match === true, `${exact} exact`);
  }
  const mm = rangeMismatches();
  const ids = mm.map((r) => r.engineId).sort();
  for (const known of ['gate_range', 'gate_att', 'gate_hold', 'gate_rel', 'comp_ratio', 'comp_knee', 'comp_att', 'comp_rel', 'gain', 'delay_ms']) {
    ok(ids.includes(known), `mismatch reported: ${known}`);
  }
  ok(ids.length === 10, 'exactly 10 mismatches', ids.join(','));
}

// ---- colours ----

ok(COLOR_HEX.length === 16, '16 desk colours');
ok(colorToHex(2) === '#5cdb5c' && colorToHex(0) === '#dc2626', 'sim defaults land on design colours');
ok(colorToHex(99) === '#64748b' && colorToHex(-1) === '#64748b', 'bad colour falls back');
ok(bankPalette('ch1').colour === '#5cdb5c', 'input bank green');
ok(bankPalette('mix').colour === '#22d3e6', 'mix bank cyan');
ok(bankPalette('grp').colour === '#2563eb' && bankPalette('fxsnd').colour === '#2563eb', 'grp/fxsnd blue');
ok(bankPalette('mtx').colour === '#c026d3' && bankPalette('main').colour === '#c026d3', 'mtx/main magenta');
ok(bankPalette('dca').colour === '#dc2626', 'dca red');

console.log(`params: ${passed} passed, ${failed} failed`);
if (failures.length) {
  for (const f of failures.slice(0, 20)) console.log('FAIL', f);
  process.exit(1);
}

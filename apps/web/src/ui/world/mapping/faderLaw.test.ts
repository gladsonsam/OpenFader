// W2a fader-law tests: both curves, exactness, and the mismatch report.
// Run: node --experimental-strip-types mapping/faderLaw.test.ts

import { DESIGN_LAW, XAIR_LAW, formatDb, lawForDriver, maxLawDeviation } from './faderLaw.ts';

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
function approx(a: number, b: number, eps = 1e-9): boolean {
  if (!Number.isFinite(a) || !Number.isFinite(b)) return a === b;
  return Math.abs(a - b) <= eps;
}

// Both laws agree exactly at every design joint >= 0.125.
for (const [f, db] of [[0.125, -50], [0.25, -30], [0.375, -20], [0.5, -10], [0.625, -5], [0.75, 0], [0.875, 5], [1, 10]] as Array<[number, number]>) {
  ok(approx(DESIGN_LAW.toDb(f), db, 1e-9), `design joint f=${f}`);
  ok(approx(XAIR_LAW.toDb(f), db, 1e-9), `xair joint f=${f}`);
}
// X-Air's own extra joint.
ok(approx(XAIR_LAW.toDb(0.0625), -60, 1e-9), 'xair 0.0625 -> -60');
// Design differs there (single 320-slope segment from the floor).
ok(approx(DESIGN_LAW.toDb(0.0625), -70, 1e-9), 'design 0.0625 -> -70 (10 dB apart)');

// Exact inverses across the whole range, both laws, both directions.
for (const law of [DESIGN_LAW, XAIR_LAW]) {
  for (let i = 0; i <= 200; i++) {
    const f = i / 200;
    const db = law.toDb(f);
    if (f === 0 || (law === DESIGN_LAW && f <= 0.003)) {
      ok(law.fromDb(db) === 0, `${law.id} floor round trip`);
      continue;
    }
    ok(approx(law.fromDb(db), f, 1e-6), `${law.id} f->db->f f=${f}`);
  }
  for (let db = -90; db <= 10; db += 2.5) {
    const f = law.fromDb(db);
    if (db <= -90) {
      ok(law.toDb(f) === -Infinity, `${law.id} bottom round trip`);
      continue;
    }
    ok(approx(law.toDb(f), db, 1e-6), `${law.id} db->f->db db=${db}`);
  }
}

// Mismatch report: worst case 10 dB at f = 0.0625, identical above 0.125.
{
  const { maxDb, atF } = maxLawDeviation(4000);
  ok(approx(maxDb, 10, 1e-6), 'max deviation is 10 dB', String(maxDb));
  ok(approx(atF, 0.0625, 1e-3), 'max deviation at f=0.0625', String(atF));
  let above = 0;
  for (let i = 0; i <= 1000; i++) {
    const f = 0.125 + (0.875 * i) / 1000;
    above = Math.max(above, Math.abs(DESIGN_LAW.toDb(f) - XAIR_LAW.toDb(f)));
  }
  ok(above < 1e-9, 'identical above f=0.125', String(above));
}

// Law selection + readout.
ok(lawForDriver('xair').id === 'xair', 'xair law');
ok(lawForDriver('dl32s').id === 'xair', 'dl32s falls back to xair (placeholder)');
ok(lawForDriver('bogus').id === 'xair', 'unknown driver falls back');
ok(lawForDriver(undefined).id === 'xair', 'no driver falls back');
ok(formatDb(-Infinity) === '-inf' && formatDb(-90) === '-inf', 'floor formats -inf');
ok(formatDb(0) === '0.0' && formatDb(-0.04) === '0.0', 'near-zero reads 0.0');
ok(formatDb(3.25) === '+3.3' && formatDb(-30) === '-30.0', 'sign + decimals');

console.log(`faderLaw: ${passed} passed, ${failed} failed`);
if (failures.length) {
  for (const f of failures.slice(0, 20)) console.log('FAIL', f);
  process.exit(1);
}

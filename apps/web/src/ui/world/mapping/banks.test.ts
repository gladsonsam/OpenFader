// W2a banks tests: deriveBanks for sim / XR18 / DL32S-like topologies.
// Run: node --experimental-strip-types mapping/banks.test.ts

import { deriveBanks } from './banks.ts';

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

// Sim topology (crates/sim/src/lib.rs SIM_TOPOLOGY).
const SIM = [
  { key: 'ch', kind: 'in', label: 'Ch 1-32', first: 0, count: 32, name_prefix: 'Ch', color_default: 2 },
  { key: 'mix', kind: 'bus', label: 'Mix 1-8', first: 32, count: 8, name_prefix: 'Mix', color_default: 6 },
  { key: 'ch2', kind: 'in', label: 'Ch 33-48', first: 40, count: 16, name_prefix: 'Ch', color_default: 2 },
  { key: 'fxret', kind: 'ret', label: 'FxRet 1-4', first: 56, count: 4, name_prefix: 'FxRet', color_default: 6 },
  { key: 'fxsnd', kind: 'fxsnd', label: 'FxSnd 1-4', first: 60, count: 4, name_prefix: 'FxSnd', color_default: 4 },
  { key: 'mtx', kind: 'mtx', label: 'Mtx 1-8', first: 64, count: 8, name_prefix: 'Mtx', color_default: 11 },
  { key: 'main', kind: 'main', label: 'Main LR', first: 72, count: 1, name_prefix: 'Main', color_default: 11 },
  { key: 'dca', kind: 'dca', label: 'DCA 1-4', first: 73, count: 4, name_prefix: 'DCA', color_default: 0 },
];

{
  const banks = deriveBanks(SIM);
  ok(banks.length === 8, 'sim yields 8 banks', String(banks.length));
  const keys = banks.map((b) => b.key);
  ok(JSON.stringify(keys) === JSON.stringify(['ch1', 'ch17', 'ch33', 'mix', 'fx', 'fxsnd', 'mtx', 'dca']), 'sim bank keys follow the design rail', keys.join(','));
  const byKey: Record<string, (typeof banks)[number]> = {};
  for (const b of banks) byKey[b.key] = b;
  ok(byKey['ch1']!.strips.length === 16 && byKey['ch1']!.strips[0] === 0 && byKey['ch1']!.strips[15] === 15, 'Ch 1-16 = strips 0..15');
  ok(byKey['ch17']!.strips[0] === 16 && byKey['ch17']!.strips[15] === 31, 'Ch 17-32 = strips 16..31');
  ok(byKey['ch33']!.strips[0] === 40 && byKey['ch33']!.strips[15] === 55, 'Ch 33-48 spans ch2');
  ok(JSON.stringify(byKey['ch33']!.families) === JSON.stringify(['ch2']), 'ch33 covers family ch2');
  ok(byKey['mix']!.strips.length === 8 && byKey['mix']!.strips[0] === 32, 'Mix bank = buses 32..39');
  ok(byKey['fx']!.label === 'FxRet 1-4' && byKey['fx']!.strips[0] === 56, 'FxRet bank');
  ok(byKey['fxsnd']!.strips[0] === 60, 'FxSnd bank');
  ok(byKey['mtx']!.strips.length === 9 && byKey['mtx']!.strips[8] === 72, 'Mtx/Main merges mtx+main');
  ok(JSON.stringify(byKey['mtx']!.families) === JSON.stringify(['mtx', 'main']), 'mtx bank families');
  ok(byKey['dca']!.strips.length === 4 && byKey['dca']!.strips[0] === 73, 'DCA bank');
  ok(byKey['ch1']!.label === 'Ch 1-16' && byKey['ch33']!.label === 'Ch 33-48', 'input bank labels');
  // Design colours.
  ok(byKey['ch1']!.colour === '#5cdb5c' && byKey['ch1']!.text === '#10240f', 'input bank green');
  ok(byKey['mix']!.colour === '#22d3e6', 'mix bank cyan');
  ok(byKey['fxsnd']!.colour === '#2563eb', 'fxsnd bank blue');
  ok(byKey['mtx']!.colour === '#c026d3', 'mtx bank magenta');
  ok(byKey['dca']!.colour === '#dc2626', 'dca bank red');
  // Full coverage, no overlaps.
  const all = banks.flatMap((b) => b.strips).sort((a, b) => a - b);
  ok(all.length === 77 && all[0] === 0 && all[76] === 76, 'sim covers all 77 strips exactly once');
}

// XR18 topology (crates/proto-xair/src/lib.rs XAIR_TOPOLOGY).
const XR18 = [
  { key: 'ch', kind: 'in', label: 'Ch 1-16', first: 0, count: 16, name_prefix: 'Ch', color_default: 2 },
  { key: 'fxret', kind: 'ret', label: 'FxRet 1-4', first: 16, count: 4, name_prefix: 'FxRet', color_default: 6 },
  { key: 'bus', kind: 'bus', label: 'Bus 1-6', first: 20, count: 6, name_prefix: 'Mix', color_default: 6 },
  { key: 'fxsnd', kind: 'fxsnd', label: 'FxSnd 1-4', first: 26, count: 4, name_prefix: 'FxSnd', color_default: 4 },
  { key: 'main', kind: 'main', label: 'Main LR', first: 30, count: 1, name_prefix: 'Main', color_default: 11 },
  { key: 'dca', kind: 'dca', label: 'DCA 1-4', first: 31, count: 4, name_prefix: 'DCA', color_default: 0 },
  { key: 'aux', kind: 'ret', label: 'Aux', first: 35, count: 1, name_prefix: 'Aux', color_default: 2 },
];

{
  const banks = deriveBanks(XR18);
  ok(banks.length === 7, 'xr18 yields 7 banks', banks.map((b) => b.key).join(','));
  const byKey: Record<string, (typeof banks)[number]> = {};
  for (const b of banks) byKey[b.key] = b;
  ok(byKey['ch1']!.label === 'Ch 1-16' && byKey['ch1']!.strips.length === 16, 'xr18 inputs');
  ok(byKey['fx']!.label === 'FxRet 1-4', 'xr18 fxret label');
  ok(byKey['mix']!.label === 'Bus 1-6', 'xr18 bus label');
  ok(byKey['fxsnd']!.label === 'FxSnd 1-4', 'xr18 fxsnd label');
  ok(byKey['mtx']!.label === 'Mtx/Main' && byKey['mtx']!.strips.length === 1, 'xr18 main folds into Mtx/Main');
  ok(byKey['dca']!.strips.length === 4, 'xr18 dca');
  ok(byKey['aux']!.label === 'Aux' && byKey['aux']!.strips.length === 1 && byKey['aux']!.strips[0] === 35, 'xr18 aux bank');
  const all = banks.flatMap((b) => b.strips).sort((a, b) => a - b);
  ok(all.length === 36 && all[35] === 35, 'xr18 covers all 36 strips');
}

// DL32S-like topology (single 32-input family).
{
  const banks = deriveBanks([
    { key: 'ch', kind: 'in', label: 'Ch 1-32', first: 0, count: 32, name_prefix: 'Ch', color_default: 2 },
  ]);
  ok(banks.length === 2, 'dl32s-like yields 2 banks');
  ok(banks[0]!.label === 'Ch 1-16' && banks[1]!.label === 'Ch 17-32', 'dl32s chunk labels');
  ok(banks[0]!.key === 'ch1' && banks[1]!.key === 'ch17', 'dl32s chunk keys');
}

// Never throws on bad input.
{
  ok(deriveBanks([]).length === 0, 'empty topology');
  const bad = deriveBanks([
    { key: '', kind: 'in', label: '', first: 0, count: 4, name_prefix: '', color_default: 0 },
    { key: 'x', kind: '', label: '', first: 0, count: 4, name_prefix: '', color_default: 0 },
    { key: 'y', kind: 'in', label: '', first: -1, count: 4, name_prefix: '', color_default: 0 },
    { key: 'z', kind: 'in', label: '', first: 0, count: 0, name_prefix: '', color_default: 0 },
  ] as never);
  ok(bad.length === 0, 'bad families skipped');
}

console.log(`banks: ${passed} passed, ${failed} failed`);
if (failures.length) {
  for (const f of failures) console.log('FAIL', f);
  process.exit(1);
}

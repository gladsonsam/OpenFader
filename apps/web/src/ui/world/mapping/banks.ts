// W2a mapping: engine topology -> design banks.
//
// Pure `deriveBanks(topology)`: groups the ordered strip-family map
// (caps `topology`: {key,kind,label,first,count,name_prefix,color_default})
// into the design's bank rail. Rules:
//   - `in` families split into 16-strip banks (`Ch 1-16`, `Ch 17-32`, …),
//     chunking across family boundaries (sim: ch[32] + ch2[16] ->
//     Ch 1-16 / Ch 17-32 / Ch 33-48, exactly the design banks).
//   - every other family becomes its own bank in topology order, except
//     `mtx`+`main` merge into one `Mtx/Main` bank (the design rail).
//   - bank keys mirror the design (`ch1`, `ch17`, `ch33`, `fx`, `mix`,
//     `grp`, `mtx`, `dca`) when the shape matches, else `<famkey>` /
//     `<famkey><start>`; labels come from global indices (`Ch 1-16`).
//   - colours come from the design bank palette (params.bankPalette), NOT
//     from color_default (the default only colours unnamed strips).
// Never throws: bad families are skipped, empty input yields no banks.

import type { WorldBank } from '../types.ts';
import { bankPalette } from './params.ts';
import type { TopologyEvt } from '../protocol.ts';

export interface TopologyFamilyInput {
  key: string;
  kind: string;
  label: string;
  first: number;
  count: number;
  name_prefix: string;
  color_default: number;
}

function cleanFamilies(input: readonly TopologyFamilyInput[]): TopologyFamilyInput[] {
  const out: TopologyFamilyInput[] = [];
  for (const f of input) {
    if (!f || typeof f.key !== 'string' || !f.key) continue;
    if (typeof f.kind !== 'string' || !f.kind) continue;
    if (!Number.isInteger(f.first) || f.first < 0) continue;
    if (!Number.isInteger(f.count) || f.count <= 0) continue;
    out.push(f);
  }
  return out.slice().sort((a, b) => a.first - b.first);
}

function bankKeyFor(famKey: string, startCh: number, kind: string): string {
  if (kind === 'in') {
    if (startCh === 0) return 'ch1';
    return 'ch' + (startCh + 1);
  }
  // The XR18 stereo aux input is its own one-strip `ret` family (`aux`):
  // it must not collide with the FX-return `fx` bank.
  if (kind === 'ret') return famKey === 'aux' ? 'aux' : 'fx';
  if (kind === 'bus') return 'mix';
  if (kind === 'fxsnd') return famKey === 'grp' ? 'grp' : 'fxsnd';
  if (kind === 'mtx' || kind === 'main') return 'mtx';
  if (kind === 'dca') return 'dca';
  return famKey + (startCh > 0 ? String(startCh + 1) : '');
}

function labelFor(kind: string, prefix: string, start: number, end: number, famLabel: string, wholeFamily: boolean): string {
  if (kind === 'mtx' || kind === 'main') return 'Mtx/Main';
  if (wholeFamily && famLabel) return famLabel;
  if (kind === 'ret') return 'FxRet';
  if (kind === 'fxsnd') return 'FxSnd';
  if (kind === 'dca') return 'DCA';
  if (kind === 'bus') return start === end ? famLabel : prefix + ' ' + (start + 1) + '-' + (end + 1);
  // inputs: `Ch 1-16` style from global indices
  const p = prefix || 'Ch';
  return start === end ? p + ' ' + (start + 1) : p + ' ' + (start + 1) + '-' + (end + 1);
}

/**
 * Derive the design bank rail from an engine topology. `chunksOf` sets the
 * input bank size (design: 16).
 */
export function deriveBanks(
  topology: readonly TopologyFamilyInput[] | readonly TopologyEvt[],
  chunksOf = 16,
): WorldBank[] {
  const fams = cleanFamilies(topology as readonly TopologyFamilyInput[]);
  const banks: WorldBank[] = [];
  // Collect input runs first (they chunk across families), then the rest in
  // topology order; mtx+main merge.
  const inRuns: Array<{ fam: TopologyFamilyInput; start: number; end: number }> = [];
  const other: TopologyFamilyInput[] = [];
  const dcaFams: TopologyFamilyInput[] = [];
  let mtxMain: TopologyFamilyInput[] = [];
  for (const f of fams) {
    if (f.kind === 'in') inRuns.push({ fam: f, start: f.first, end: f.first + f.count - 1 });
    else if (f.kind === 'mtx' || f.kind === 'main') mtxMain.push(f);
    else if (f.kind === 'dca') dcaFams.push(f); // DCA masters ride last (design rail)
    else other.push(f);
  }

  // Input banks: walk the global input index space in chunks.
  const allIn: number[] = [];
  const inFams: TopologyFamilyInput[] = [];
  for (const r of inRuns) {
    for (let c = r.start; c <= r.end; c++) allIn.push(c);
    inFams.push(r.fam);
  }
  for (let i = 0; i < allIn.length; i += chunksOf) {
    const slice = allIn.slice(i, i + chunksOf);
    const start = slice[0]!;
    const end = slice[slice.length - 1]!;
    const key = bankKeyFor('ch', i, 'in');
    const pal = bankPalette(key);
    const famsCovered = [...new Set(inFams.filter((f) => f.first <= end && start <= f.first + f.count - 1).map((f) => f.key))];
    banks.push({
      key,
      // Ordinal labels (input #i+1..), not global strips: buses may sit
      // between input families (sim mix at 32..39).
      label: labelFor('in', inFams[0]?.name_prefix ?? 'Ch', i, i + slice.length - 1, '', false),
      colour: pal.colour,
      text: pal.text,
      strips: slice,
      families: famsCovered,
    });
  }

  const pushFamBank = (f: TopologyFamilyInput): void => {
    const start = f.first;
    const end = f.first + f.count - 1;
    const key = bankKeyFor(f.key, start, f.kind);
    const pal = bankPalette(key);
    const strips: number[] = [];
    for (let c = start; c <= end; c++) strips.push(c);
    banks.push({
      key,
      label: labelFor(f.kind, f.name_prefix, start, end, f.label, true),
      colour: pal.colour,
      text: pal.text,
      strips,
      families: [f.key],
    });
  };

  for (const f of other) pushFamBank(f);

  if (mtxMain.length) {
    const strips: number[] = [];
    for (const f of mtxMain) for (let c = f.first; c < f.first + f.count; c++) strips.push(c);
    const pal = bankPalette('mtx');
    banks.push({
      key: 'mtx',
      label: 'Mtx/Main',
      colour: pal.colour,
      text: pal.text,
      strips,
      families: mtxMain.map((f) => f.key),
    });
  }
  for (const f of dcaFams) pushFamBank(f);
  return banks;
}

// Detail-panel control definitions.
// Ranges mirror crates/proto-dl32s address limits (engineering units).
// Ported from ui/app.js — keep the two in sync.

export const SECTIONS = [
  { title: 'Input', fields: [
    { f: 'gain', label: 'gain', unit: 'dB', type: 'f', min: -20, max: 60, step: 0.5, def: 0 },
    { f: 'phantom', label: '+48V', type: 'b' },
    { f: 'pan', label: 'pan', unit: 'L/R', type: 'f', min: -1, max: 1, step: 0.01, def: 0 },
  ]},
  { title: 'Filter / dynamics', fields: [
    { f: 'hpf_on', label: 'hpf on', type: 'b' },
    { f: 'hpf_freq', label: 'hpf', unit: 'Hz', type: 'f', min: 20, max: 400, step: 1, def: 80 },
    { f: 'gate_on', label: 'gate on', type: 'b' },
    { f: 'comp_on', label: 'comp on', type: 'b' },
    { f: 'eq_on', label: 'eq on', type: 'b' },
  ]},
  { title: 'EQ bands 1-4 (gain / freq / Q)', fields: [
    ...[1, 2, 3, 4].flatMap((b) => ([
      { f: `eq${b}_gain`, label: `eq${b} gain`, unit: 'dB', type: 'f', min: -20, max: 20, step: 0.5, def: 0 },
      { f: `eq${b}_freq`, label: `eq${b} freq`, unit: 'Hz', type: 'logf', min: 20, max: 20000, def: b === 1 ? 100 : b === 4 ? 12000 : 1000 * b },
      { f: `eq${b}_q`, label: `eq${b} Q`, type: 'f', min: 0.1, max: 20, step: 0.1, def: 1 },
      ...(b === 1 ? [] : [{ f: `eq${b}_type`, label: `eq${b} type`, type: 'enum', opts: [0, 1, 2, 3], def: 0 }]),
    ])),
  ]},
  { title: 'Aux sends 1-8', fields: [
    ...Array.from({ length: 8 }, (_, i) => ([
      { f: `aux${i + 1}_level`, label: `aux${i + 1}`, unit: 'dB', type: 'f', min: -120, max: 10, step: 0.5, def: -120 },
      { f: `aux${i + 1}_mute`, label: `m${i + 1}`, type: 'b' },
    ])).flat(),
  ]},
  { title: 'FX sends 1-4', fields: [
    ...Array.from({ length: 4 }, (_, i) => ([
      { f: `fx${i + 1}_level`, label: `fx${i + 1}`, unit: 'dB', type: 'f', min: -120, max: 10, step: 0.5, def: -120 },
      { f: `fx${i + 1}_mute`, label: `m${i + 1}`, type: 'b' },
    ])).flat(),
  ]},
];

export const logToT = (v, min, max) => Math.log(v / min) / Math.log(max / min);
export const logFromT = (t, min, max) => min * Math.pow(max / min, t);

// Channel strip presets: built-in library + capture/apply core.
//
// A preset is gain/EQ/dynamics starter values only — never fader, mute,
// pan, sends, name, colour or icon. 48V phantom is never captured and never
// recalled (safety: a preset must not energise an input). Recall goes
// through the same ui.do setters as user gestures (see applyPreset), so
// fixture edits and the live write-through bridge (uiStore forward()) apply
// with identical discipline; unsupported desk leaves are refused by the
// bridge and reported here, never faked.
//
// Rune-free and DOM-free: node-testable with a fixture channel + fake ops.
// The Svelte card (PresetsCard) and gallery are thin shells over this file.

import { spec, toN, type Channel, type EqBandType } from '../fixtures/designData.ts';
import { dynSpecKey } from '../state/logic/inputDyn.ts';
import data from './channelPresets.default.json' with { type: 'json' };

export interface PresetInput {
  gain?: number;
  trim?: number;
  invert?: boolean;
  delayOn?: boolean;
  delay?: number;
}

export interface PresetHpf {
  on: boolean | null;
  f: number | null;
  slope: 12 | 24 | null;
}

export interface PresetBand {
  type: EqBandType;
  f: number;
  g: number;
  q: number;
  on: boolean;
}

export interface PresetEq {
  on: boolean;
  hpf: PresetHpf;
  bands: PresetBand[];
}

export interface PresetGate {
  on: boolean;
  thr: number;
  range: number;
  att: number;
  hold: number;
  rel: number;
}

export interface PresetComp {
  on: boolean;
  thr: number;
  ratio: number;
  att: number;
  rel: number;
  knee: number;
  makeup: number;
}

export type PresetSource = 'builtin' | 'user';

/** Sections are optional: a user preset stores only what the source strip had. */
export interface ChannelPreset {
  id: string;
  label: string;
  blurb?: string;
  source: PresetSource;
  input?: PresetInput;
  eq?: PresetEq;
  gate?: PresetGate;
  comp?: PresetComp;
}

/** The store surface applyPreset needs (satisfied structurally by ui.do). */
export interface PresetDo {
  setInputKnob(id: string, key: 'gain' | 'trim' | 'delay', v: number): void;
  setEqParam(id: string, band: number, param: 'f' | 'g' | 'q', v: number): void;
  setEqBandType(id: string, band: number, t: EqBandType): void;
  setHpfSlope(id: string, s: 12 | 24): void;
  toggleEqOn(id: string): void;
  toggleEqBand(id: string, band: number): void;
  toggleDelayOn(id: string): void;
  toggleInvert(id: string): void;
  setDynParam(id: string, which: 'gate' | 'comp', prop: string, n: number): void;
  toggleDyn(id: string, which: 'gate' | 'comp'): void;
}

export interface ApplyReport {
  applied: string[];
  skipped: string[];
  refused: string | null;
}

// ---------- built-in library (data; editable JSON, no hard-coded values) ----------

function isBand(b: unknown): b is PresetBand {
  if (typeof b !== 'object' || b === null) return false;
  const r = b as Record<string, unknown>;
  return (
    ['LCut', 'LS', 'PEQ', 'VEQ', 'HS', 'HCut'].includes(r['type'] as string) &&
    [r['f'], r['g'], r['q']].every(v => typeof v === 'number' && Number.isFinite(v)) &&
    typeof r['on'] === 'boolean'
  );
}

/** Validate one raw preset object; null when it is not a usable preset. */
export function sanitizePreset(raw: unknown, source: PresetSource): ChannelPreset | null {
  if (typeof raw !== 'object' || raw === null) return null;
  const r = raw as Record<string, unknown>;
  if (typeof r['id'] !== 'string' || !r['id'] || typeof r['label'] !== 'string' || !r['label']) return null;
  const out: ChannelPreset = { id: r['id'], label: r['label'].slice(0, 32), source };
  if (typeof r['blurb'] === 'string' && r['blurb']) out.blurb = r['blurb'].slice(0, 120);
  const inp = r['input'] as Record<string, unknown> | undefined;
  if (inp && typeof inp === 'object') {
    const input: PresetInput = {};
    for (const k of ['gain', 'trim', 'delay'] as const) if (typeof inp[k] === 'number' && Number.isFinite(inp[k])) input[k] = inp[k] as number;
    for (const k of ['invert', 'delayOn'] as const) if (typeof inp[k] === 'boolean') input[k] = inp[k] as boolean;
    if (Object.keys(input).length) out.input = input;
  }
  const eq = r['eq'] as Record<string, unknown> | undefined;
  if (eq && typeof eq === 'object' && typeof eq['on'] === 'boolean') {
    const hpf = eq['hpf'] as Record<string, unknown> | undefined;
    const bands = eq['bands'];
    if (
      hpf && typeof hpf === 'object' && (hpf['on'] === null || typeof hpf['on'] === 'boolean') &&
      (hpf['f'] === null || (typeof hpf['f'] === 'number' && Number.isFinite(hpf['f']))) &&
      (hpf['slope'] === null || hpf['slope'] === 12 || hpf['slope'] === 24) &&
      Array.isArray(bands) && (bands.length === 4 || bands.length === 6) && bands.every(isBand)
    ) {
      out.eq = {
        on: eq['on'] as boolean,
        hpf: { on: hpf['on'] as boolean | null, f: hpf['f'] as number | null, slope: hpf['slope'] as 12 | 24 | null },
        bands: (bands as PresetBand[]).map(b => ({ ...b })),
      };
    }
  }
  const gate = r['gate'] as Record<string, unknown> | undefined;
  if (
    gate && typeof gate === 'object' && typeof gate['on'] === 'boolean' &&
    ['thr', 'range', 'att', 'hold', 'rel'].every(k => typeof gate[k] === 'number' && Number.isFinite(gate[k]))
  ) {
    out.gate = {
      on: gate['on'] as boolean,
      thr: gate['thr'] as number,
      range: gate['range'] as number,
      att: gate['att'] as number,
      hold: gate['hold'] as number,
      rel: gate['rel'] as number,
    };
  }
  const comp = r['comp'] as Record<string, unknown> | undefined;
  if (
    comp && typeof comp === 'object' && typeof comp['on'] === 'boolean' &&
    ['thr', 'ratio', 'att', 'rel', 'knee', 'makeup'].every(k => typeof comp[k] === 'number' && Number.isFinite(comp[k]))
  ) {
    out.comp = {
      on: comp['on'] as boolean,
      thr: comp['thr'] as number,
      ratio: comp['ratio'] as number,
      att: comp['att'] as number,
      rel: comp['rel'] as number,
      knee: comp['knee'] as number,
      makeup: comp['makeup'] as number,
    };
  }
  // A preset with no sections recalls nothing: reject it.
  if (!out.input && !out.eq && !out.gate && !out.comp) return null;
  return out;
}

/** Built-in starters from the shipped JSON (validated; invalid rows dropped). */
export function builtinPresets(): ChannelPreset[] {
  const list = (data as { presets?: unknown }).presets;
  if (!Array.isArray(list)) return [];
  const out: ChannelPreset[] = [];
  for (const raw of list) {
    const p = sanitizePreset(raw, 'builtin');
    if (p) out.push(p);
  }
  return out;
}

// ---------- capture (channel state -> preset; phantom excluded by design) ----------

let userSeq = 0;

/** Capture the tone leaves of a channel. Phantom is deliberately dropped. */
export function captureChannel(ch: Channel, label: string): ChannelPreset {
  const preset: ChannelPreset = {
    id: 'user-' + Date.now().toString(36) + '-' + (userSeq++).toString(36),
    label: label.slice(0, 32) || ch.name,
    source: 'user',
  };
  if (ch.input) {
    const input: PresetInput = {};
    for (const key of ['gain', 'trim', 'delay'] as const) if ((!ch.input.live || ch.input.live.known[key]) && Number.isFinite(ch.input[key])) input[key] = ch.input[key];
    for (const key of ['invert', 'delayOn'] as const) if (!ch.input.live || ch.input.live.known[key]) input[key] = ch.input[key];
    if (Object.keys(input).length) preset.input = input;
  }
  if (ch.eq && ch.eq.on !== null && ch.eq.bands.length > 0 && ch.eq.bands.every(isBand)) {
    preset.eq = {
      on: ch.eq.on,
      hpf: { on: ch.eq.hpf.on, f: Number.isFinite(ch.eq.hpf.f) ? ch.eq.hpf.f : null, slope: ch.eq.hpf.slope },
      bands: ch.eq.bands.map(b => ({ type: b.type!, f: b.f, g: b.g, q: b.q, on: b.on! })),
    };
  }
  if (ch.dyn) {
    const { live: gl, raw: _gr, ...gate } = ch.dyn.gate, { live: cl, raw: _cr, ...comp } = ch.dyn.comp;
    if ((!gl || gl.known.on) && [gate.thr, gate.range, gate.att, gate.hold, gate.rel].every(Number.isFinite)) preset.gate = gate;
    if ((!cl || cl.known.on) && [comp.thr, comp.ratio, comp.att, comp.rel, comp.knee, comp.makeup].every(Number.isFinite)) preset.comp = comp;
  }
  return preset;
}

// ---------- honesty: preset leaves -> engine param ids ----------

export interface PresetLeaf {
  /** Stable leaf key for reports (e.g. "eq.band2.g", "comp.knee"). */
  leaf: string;
  /** Engine caps id, or null when the desk has no such leaf (fixture-only). */
  engine: string | null;
}

/**
 * Display-only leaf map. The write path (ui.do -> engineBridge) is
 * authoritative at write time and refuses unknown ids; this table only
 * drives the "stays local" honesty notes. Per-band bypass has no engine
 * leaf (the desk has a global EQ switch only).
 */
export function presetLeaves(p: ChannelPreset): PresetLeaf[] {
  const out: PresetLeaf[] = [];
  if (p.input) {
    for (const [key, id] of Object.entries({ gain: 'gain', trim: 'trim', invert: 'polarity', delayOn: 'delay_on', delay: 'delay_ms' })) if (p.input[key as keyof PresetInput] !== undefined) out.push({ leaf: `input.${key}`, engine: id });
  }
  if (p.eq) {
    out.push({ leaf: 'eq.on', engine: 'eq_on' });
    out.push({ leaf: 'eq.hpf.on', engine: 'hpf_on' });
    out.push({ leaf: 'eq.hpf.f', engine: 'hpf_freq' });
    out.push({ leaf: 'eq.hpf.slope', engine: 'hpf_slope' });
    p.eq.bands.forEach((_, i) => {
      const n = i + 1;
      out.push({ leaf: `eq.band${n}.f`, engine: `eq${n}_freq` });
      out.push({ leaf: `eq.band${n}.g`, engine: `eq${n}_gain` });
      out.push({ leaf: `eq.band${n}.q`, engine: `eq${n}_q` });
      out.push({ leaf: `eq.band${n}.type`, engine: `eq${n}_type` });
      out.push({ leaf: `eq.band${n}.on`, engine: null });
    });
  }
  if (p.gate) {
    for (const k of ['on', 'thr', 'range', 'att', 'hold', 'rel'] as const) {
      out.push({ leaf: `gate.${k}`, engine: k === 'on' ? 'gate_on' : `gate_${k}` });
    }
  }
  if (p.comp) {
    for (const k of ['on', 'thr', 'ratio', 'att', 'rel', 'knee', 'makeup'] as const) {
      out.push({ leaf: `comp.${k}`, engine: k === 'on' ? 'comp_on' : `comp_${k}` });
    }
  }
  return out;
}

export interface SupportReport {
  /** Leaves with no engine leaf at all (fixture-only by design). */
  fixtureOnly: string[];
  /** Leaves the desk refuses (caps): recalled to the fixture model only. */
  deskRefused: string[];
}

/**
 * Honest support diff. `supports` is null in fixture mode (no desk to
 * disagree with: nothing is refused). `stripMapped` is false when the
 * strip has no desk equivalent at all (then every engine leaf is refused).
 */
export function diffSupport(
  p: ChannelPreset,
  supports: ((engineId: string) => boolean) | null,
  stripMapped: boolean,
): SupportReport {
  const fixtureOnly: string[] = [];
  const deskRefused: string[] = [];
  for (const l of presetLeaves(p)) {
    if (l.engine === null) {
      fixtureOnly.push(l.leaf);
      continue;
    }
    if (supports && (!stripMapped || !supports(l.engine))) deskRefused.push(l.leaf);
  }
  return { fixtureOnly, deskRefused };
}

// ---------- apply (through ui.do setters: fixture + live bridge) ----------

function dynN(which: 'gate' | 'comp', prop: string, value: number): number {
  return toN(spec(dynSpecKey(which, prop)), value);
}

/**
 * Recall a preset onto a channel through the store setters (the same calls
 * user gestures make, so the live bridge forwards each leaf when online and
 * fixture mode just edits the model). Toggles are absolute: only flipped
 * when the channel differs from the preset. Sections the channel lacks
 * (no preamp on a bus, no EQ/dyn on a DCA) are skipped and reported —
 * never faked. Locked stores refuse the whole recall.
 */
export function applyPreset(
  ops: PresetDo,
  id: string,
  ch: Channel,
  p: ChannelPreset,
  locked: boolean,
): ApplyReport {
  const applied: string[] = [];
  const skipped: string[] = [];
  if (locked) {
    return { applied, skipped: presetLeaves(p).map(l => l.leaf), refused: 'locked' };
  }
  if (p.input) {
    if (ch.input) {
      for (const key of ['gain', 'trim', 'delay'] as const) {
        const v = p.input[key];
        if (v === undefined) continue;
        if (ch.input.live && !ch.input.live.supported[key]) { skipped.push(`input.${key}`); continue; }
        ops.setInputKnob(id, key, v); applied.push(`input.${key}`);
      }
      if (p.input.delayOn !== undefined && (!ch.input.live || ch.input.live.known.delayOn) && ch.input.delayOn !== p.input.delayOn) {
        ops.toggleDelayOn(id);
        applied.push('input.delayOn');
      }
      if (p.input.invert !== undefined && (!ch.input.live || ch.input.live.known.invert) && ch.input.invert !== p.input.invert) {
        ops.toggleInvert(id);
        applied.push('input.invert');
      }
    } else {
      skipped.push('input (no preamp on this strip)');
    }
  }
  if (p.eq) {
    if (ch.eq) {
      if (ch.eq.on !== p.eq.on) {
        ops.toggleEqOn(id);
        applied.push('eq.on');
      }
      if (p.eq.hpf.on !== null && ch.eq.hpf.on !== null && ch.eq.hpf.on !== p.eq.hpf.on) {
        ops.toggleEqBand(id, 0);
        applied.push('eq.hpf.on');
      }
      if (p.eq.hpf.f !== null) {
        ops.setEqParam(id, 0, 'f', p.eq.hpf.f);
        applied.push('eq.hpf.f');
      }
      if (p.eq.hpf.slope !== null && ch.eq.hpf.slope !== p.eq.hpf.slope) {
        ops.setHpfSlope(id, p.eq.hpf.slope);
        applied.push('eq.hpf.slope');
      }
      p.eq.bands.forEach((b, i) => {
        const cur = ch.eq!.bands[i];
        ops.setEqParam(id, i + 1, 'f', b.f);
        ops.setEqParam(id, i + 1, 'g', b.g);
        ops.setEqParam(id, i + 1, 'q', b.q);
        ops.setEqBandType(id, i + 1, b.type);
        applied.push(`eq.band${i + 1}.f`, `eq.band${i + 1}.g`, `eq.band${i + 1}.q`, `eq.band${i + 1}.type`);
        if (cur && cur.on !== b.on) {
          ops.toggleEqBand(id, i + 1);
          applied.push(`eq.band${i + 1}.on`);
        }
      });
    } else {
      skipped.push('eq (no EQ on this strip)');
    }
  }
  if (p.gate) {
    if (ch.dyn) {
      if (ch.dyn.gate.on !== p.gate.on) {
        ops.toggleDyn(id, 'gate');
        applied.push('gate.on');
      }
      (['thr', 'range', 'att', 'hold', 'rel'] as const).forEach(k => {
        ops.setDynParam(id, 'gate', k, dynN('gate', k, p.gate![k]));
        applied.push(`gate.${k}`);
      });
    } else {
      skipped.push('gate (no dynamics on this strip)');
    }
  }
  if (p.comp) {
    if (ch.dyn) {
      if (ch.dyn.comp.on !== p.comp.on) {
        ops.toggleDyn(id, 'comp');
        applied.push('comp.on');
      }
      (['thr', 'ratio', 'att', 'rel', 'knee', 'makeup'] as const).forEach(k => {
        ops.setDynParam(id, 'comp', k, dynN('comp', k, p.comp![k]));
        applied.push(`comp.${k}`);
      });
    } else {
      skipped.push('comp (no dynamics on this strip)');
    }
  }
  return { applied, skipped, refused: null };
}

/** One-line summary for the picker rows (blurb preferred, derived fallback). */
export function summarizePreset(p: ChannelPreset): string {
  if (p.blurb) return p.blurb;
  const bits: string[] = [];
  if (p.eq) {
    bits.push(p.eq.on ? 'EQ in' : 'EQ out');
    if (p.eq.hpf.on) bits.push('HPF ' + p.eq.hpf.f);
  }
  if (p.comp) bits.push(p.comp.on ? 'comp ' + p.comp.ratio + ':1' : 'comp out');
  if (p.gate?.on) bits.push('gate in');
  return bits.join(' · ') || 'tone starter';
}

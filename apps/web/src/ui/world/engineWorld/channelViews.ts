// Channel detail views: design-model Eq/Dyn/Sends/Input builders over engine
// ids, plus the design-side write helpers (used by the W1 binding and the
// write-through bridge). Pure over a narrow ViewSource so the builders are
// node-testable without a transport; EngineWorld delegates with itself.
//
// Run: node --experimental-strip-types apps/web/src/ui/world/engineWorld/channelViews.test.ts

import type { Eq, EqBandType, EqRange } from '../../fixtures/designData.ts';
import { lawForDriver } from '../mapping/faderLaw.ts';
import {
  design01ToSendDb,
  designEqTypeToEngine,
  designPanToEngine,
  designSlopeToEngine,
  engineEqTypeToDesign,
  engineSlopeToDesign,
  preToTap,
  sendDbToDesign01,
  tapToPre,
} from '../mapping/params.ts';
import type {
  DynView,
  EqView,
  FieldView,
  InputView,
  ParamStatus,
  ParamValue,
  SendView,
} from '../types.ts';

/** Narrow read surface the view builders need (EngineWorld satisfies it). */
export interface ViewSource {
  param(ch: number, id: string): ParamValue | null;
  status(ch: number, id: string): ParamStatus;
  supports(id: string, ch?: number): boolean;
  /** A confirmed (non-optimistic) value exists for `ch:id`. */
  confirmed(ch: number, id: string): boolean;
  def?(id: string): { min: number | null; max: number | null } | null;
  readonly driver: string;
}

/** Narrow write surface the design write helpers need. */
export interface ViewSink {
  setParam(ch: number, id: string, v: ParamValue): boolean;
  def?(id: string): { min: number | null; max: number | null } | null;
  readonly driver: string;
}

function field<T>(value: T | null, status: ParamStatus): FieldView<T> {
  return { value, status };
}

const numOrNull = (v: ParamValue | null): number | null => (typeof v === 'number' ? v : null);
const boolOrNull = (v: ParamValue | null): boolean | null => (typeof v === 'boolean' ? v : null);

  // ---- channel detail views ----

export function buildChannelEq(src: ViewSource, ch: number): EqView {
    const st = (id: string): FieldView<number> => field(numOrNull(src.param(ch, id)), src.status(ch, id));
    const stb = (id: string): FieldView<boolean> => field(boolOrNull(src.param(ch, id)), src.status(ch, id));
    const band = (n: number): EqView['bands'][number] => {
      const raw = numOrNull(src.param(ch, `eq${n}_type`));
      return {
        type: raw === null ? null : engineEqTypeToDesign(raw) || null,
        typeStatus: src.status(ch, `eq${n}_type`),
        f: st(`eq${n}_freq`),
        g: st(`eq${n}_gain`),
        q: st(`eq${n}_q`),
        // The engine has no per-band bypass: band.on mirrors the global EQ
        // switch (a single-band toggle is unsupported -> W3 fallback).
        on: { value: stb('eq_on').value, status: src.status(ch, 'eq_on') },
        rawType: raw,
      };
    };
    const slopeRaw = numOrNull(src.param(ch, 'hpf_slope'));
    const bands: EqView['bands'] = [];
    for (let n = 1; n <= 6; n++) {
      if (src.supports(`eq${n}_freq`, ch) || src.confirmed(ch, `eq${n}_freq`)) bands.push(band(n));
    }
    return {
      on: stb('eq_on'),
      hpf: {
        on: stb('hpf_on'),
        f: st('hpf_freq'),
        slope: field(slopeRaw === null ? null : engineSlopeToDesign(slopeRaw), src.status(ch, 'hpf_slope')),
      },
      bands,
      band5: bands[4] ?? null,
      band6: bands[5] ?? null,
    };
  }

/** Project the engine view into the existing shared display model. Pending
 * values come from param(), exactly like other EngineWorld views, so an edit
 * stays optimistic until echo/expiry. Never substitute fixture defaults. */
export function buildDisplayEq(src: ViewSource, ch: number, available = true): Eq {
  const view = buildChannelEq(src, ch);
  const known = <T>(f: FieldView<T>): T | null =>
    available && (f.status === 'known' || f.status === 'pending') ? f.value : null;
  const range = (id: string): EqRange | null => {
    const d = src.def?.(id);
    return src.supports(id, ch) && d?.min != null && d.max != null ? [d.min, d.max] : null;
  };
  return {
    live: true,
    on: known(view.on),
    hpf: {
      on: known(view.hpf.on), f: known(view.hpf.f) ?? NaN,
      slope: known(view.hpf.slope),
      supported: src.supports('hpf_on', ch) || src.supports('hpf_freq', ch),
      range: range('hpf_freq'),
      slopeEditable: src.supports('hpf_slope', ch) && known(view.hpf.slope) !== null,
    },
    bands: view.bands.map((b, i) => {
      const n = i + 1;
      const d = src.def?.(`eq${n}_type`);
      const types: EqBandType[] = [];
      if (d?.min != null && d.max != null && src.supports(`eq${n}_type`, ch)) {
        for (let t = d.min; t <= d.max; t++) {
          const type = engineEqTypeToDesign(t);
          if (type) types.push(type);
        }
      }
      return {
        type: available && (b.typeStatus === 'known' || b.typeStatus === 'pending') ? b.type : null,
        f: known(b.f) ?? NaN, g: known(b.g) ?? NaN, q: known(b.q) ?? NaN,
        on: known(b.on), toggleable: false, types,
        ranges: { f: range(`eq${n}_freq`), g: range(`eq${n}_gain`), q: range(`eq${n}_q`) },
      };
    }),
  };
}

export function buildChannelDyn(src: ViewSource, ch: number): DynView {
    const st = (id: string): FieldView<number> => field(numOrNull(src.param(ch, id)), src.status(ch, id));
    const stb = (id: string): FieldView<boolean> => field(boolOrNull(src.param(ch, id)), src.status(ch, id));
    return {
      gate: {
        on: stb('gate_on'),
        thr: st('gate_thr'),
        range: st('gate_range'),
        att: st('gate_att'),
        hold: st('gate_hold'),
        rel: st('gate_rel'),
      },
      // NOTE: engine comp_hold has no design knob; it stays param-level
      // (param()/status()) until W3 adds a control or collapses it.
      comp: {
        on: stb('comp_on'),
        thr: st('comp_thr'),
        ratio: st('comp_ratio'),
        att: st('comp_att'),
        rel: st('comp_rel'),
        knee: st('comp_knee'),
        makeup: st('comp_makeup'),
      },
    };
  }

export function buildChannelSends(src: ViewSource, ch: number): SendView[] {
    const out: SendView[] = [];
    for (let n = 1; n <= 16; n++) {
      const db = numOrNull(src.param(ch, `send${n}_level`));
      const tap = numOrNull(src.param(ch, `send${n}_tap`));
      // Group-on is membership in the subgroup pickoff, never a send mute.
      const grpon = boolOrNull(src.param(ch, `send${n}_grpon`));
      const legacyOn = boolOrNull(src.param(ch, `send${n}_on`));
      out.push({
        value: field(db === null ? null : sendDbToDesign01(db, lawForDriver(src.driver)), src.status(ch, `send${n}_level`)),
        pre: field(tap === null ? null : tapToPre(tap, src.driver), src.status(ch, `send${n}_tap`)),
        on: field(legacyOn, src.status(ch, `send${n}_on`)),
        grpon: field(grpon, src.status(ch, `send${n}_grpon`)),
        tap: field(tap, src.status(ch, `send${n}_tap`)),
      });
    }
    return out;
  }

export function buildChannelInput(src: ViewSource, ch: number): InputView {
    const st = (id: string): FieldView<number> => field(numOrNull(src.param(ch, id)), src.status(ch, id));
    const stb = (id: string): FieldView<boolean> => field(boolOrNull(src.param(ch, id)), src.status(ch, id));
    return {
      gain: st('gain'),
      trim: st('trim'),
      phantom: stb('phantom'),
      invert: {
        value: stb('polarity').value,
        status: src.status(ch, 'polarity'),
      },
      delayOn: stb('delay_on'),
      delayMs: st('delay_ms'),
    };
  }

export function writeEqBandType(sink: ViewSink, ch: number, band: number, t: EqBandType): boolean {
    return sink.setParam(ch, `eq${band}_type`, designEqTypeToEngine(t));
  }
export function writeSlope(sink: ViewSink, ch: number, s: 12 | 24): boolean {
    return sink.setParam(ch, 'hpf_slope', designSlopeToEngine(s));
  }
export function writePan(sink: ViewSink, ch: number, p: number): boolean {
    return sink.setParam(ch, 'pan', designPanToEngine(p));
  }
export function writeSendLevel(sink: ViewSink, ch: number, n: number, v01: number): boolean {
    return sink.setParam(ch, `send${n}_level`, design01ToSendDb(v01, lawForDriver(sink.driver)));
  }
export function writeSendPre(sink: ViewSink, ch: number, n: number, pre: boolean): boolean {
    return sink.setParam(ch, `send${n}_tap`, preToTap(pre, sink.driver));
  }

/** Display metadata travels with the actual fixture-backed controls. Values
 * absent from the desk stay unknown; capability defaults are never readings. */
export function displayFields(src: ViewSource, ch: number, fields: Record<string, string>): import('../../fixtures/designData.ts').LiveFields {
  const supported: Record<string, boolean> = {}, known: Record<string, boolean> = {};
  const ranges: Record<string, [number, number]> = {};
  for (const [prop, id] of Object.entries(fields)) {
    supported[prop] = src.supports(id, ch);
    known[prop] = supported[prop] && src.param(ch, id) !== null;
    const d = supported[prop] ? src.def?.(id) : null;
    if (d && d.min !== null && d.max !== null) ranges[prop] = [d.min, d.max];
  }
  return { driver: src.driver, supported, known, ranges };
}

export function buildDisplayDyn(src: ViewSource, ch: number): import('../../fixtures/designData.ts').Dyn | null {
  if (!src.supports('gate_on', ch) && !src.supports('comp_on', ch)) return null;
  const build = (kind: 'gate' | 'comp', props: string[]): Record<string, unknown> => {
    const fields = Object.fromEntries(['on', ...props].map(p => [p, `${kind}_${p}`]));
    const live = displayFields(src, ch, fields);
    if (kind === 'comp' && src.driver === 'xair') live.choices = { ratio: [1.1, 1.3, 1.5, 2, 2.5, 3, 4, 5, 7, 10, 20, 100] };
    const advanced = kind === 'gate' ? ['mode', 'key_src', 'filter_on', 'filter_type', 'filter_freq'] : ['hold', 'mode', 'det', 'env', 'mix', 'auto', 'key_src', 'filter_on', 'filter_type', 'filter_freq'];
    const raw = Object.fromEntries([...Object.keys(fields), ...advanced].filter(p => src.supports(`${kind}_${p}`, ch)).map(p => [p, src.param(ch, `${kind}_${p}`)]));
    return Object.fromEntries([['live', live], ['raw', raw], ...Object.entries(fields).map(([p, id]) => {
      const v = live.known[p] ? src.param(ch, id) : null;
      return [p, p === 'on' ? v === true : typeof v === 'number' ? v : NaN];
    })]);
  };
  return {
    gate: build('gate', ['thr', 'range', 'att', 'hold', 'rel']) as unknown as import('../../fixtures/designData.ts').Gate,
    comp: build('comp', ['thr', 'ratio', 'att', 'rel', 'knee', 'makeup']) as unknown as import('../../fixtures/designData.ts').Comp,
  };
}

export function buildDisplayInput(src: ViewSource, ch: number): import('../../fixtures/designData.ts').ChannelInput | null {
  const fields = { gain: 'gain', trim: 'trim', phantom: 'phantom', invert: 'polarity', delayOn: 'delay_on', delay: 'delay_ms' };
  const live = displayFields(src, ch, fields);
  if (!Object.values(live.supported).some(Boolean)) return null;
  const n = (p: keyof typeof fields): number => live.known[p] && typeof src.param(ch, fields[p]) === 'number' ? src.param(ch, fields[p]) as number : NaN;
  const b = (p: keyof typeof fields): boolean => live.known[p] && src.param(ch, fields[p]) === true;
  return { live, gain: n('gain'), trim: n('trim'), phantom: b('phantom'), invert: b('invert'), delayOn: b('delayOn'), delay: n('delay') };
}

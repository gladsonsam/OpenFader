// Channel detail views: design-model Eq/Dyn/Sends/Input builders over engine
// ids, plus the design-side write helpers (used by the W1 binding and the
// write-through bridge). Pure over a narrow ViewSource so the builders are
// node-testable without a transport; EngineWorld delegates with itself.
//
// Run: node --experimental-strip-types apps/web/src/ui/world/engineWorld/channelViews.test.ts

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
  readonly driver: string;
}

/** Narrow write surface the design write helpers need. */
export interface ViewSink {
  setParam(ch: number, id: string, v: ParamValue): boolean;
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
        type: raw === null ? 'PEQ' : engineEqTypeToDesign(raw),
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
    return {
      on: stb('eq_on'),
      hpf: {
        on: stb('hpf_on'),
        f: st('hpf_freq'),
        slope: field(slopeRaw === null ? null : engineSlopeToDesign(slopeRaw), src.status(ch, 'hpf_slope')),
      },
      bands: [band(1), band(2), band(3), band(4)],
      band5: src.supports('eq5_gain', ch) || src.confirmed(ch, 'eq5_gain') ? band(5) : null,
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
      // Group-on is the desk send-mute leaf (`sendN_grpon` on X-Air; the
      // legacy `sendN_on` id on older sim caps). Prefer grpon, fall back
      // to on so both drivers report honestly.
      const grpon = boolOrNull(src.param(ch, `send${n}_grpon`));
      const legacyOn = boolOrNull(src.param(ch, `send${n}_on`));
      // Prefer the desk leaf where the driver has it (xair grpon), else
      // the legacy id — status follows the same pick so an unconfirmed
      // grpon reads "unknown", never the legacy "unsupported".
      const useGrpon = src.supports(`send${n}_grpon`, ch);
      const onVal = useGrpon ? grpon : (legacyOn ?? grpon);
      const onStatus = useGrpon ? src.status(ch, `send${n}_grpon`) : src.status(ch, `send${n}_on`);
      out.push({
        value: field(db === null ? null : sendDbToDesign01(db, lawForDriver(src.driver)), src.status(ch, `send${n}_level`)),
        pre: field(tap === null ? null : tapToPre(tap), src.status(ch, `send${n}_tap`)),
        on: field(onVal, onStatus),
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

export function writeEqBandType(sink: ViewSink, ch: number, band: number, t: 'LS' | 'PEQ' | 'HS'): boolean {
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
    return sink.setParam(ch, `send${n}_tap`, preToTap(pre));
  }

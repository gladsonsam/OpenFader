// W2a protocol: typed parsers/validators for EVERY engine frame and command.
//
// Sources: crates/engine/README.md (frames, caps/topology/scopes/meter_taps,
// params/focus, binary meter/GR frames, hello, stats), crates/engine/
// src/protocol.rs (Cmd/Event shapes incl. R2 devices/link/info/scenes/fx/
// routing/offline_diff/taps), crates/engine/tests/golden/*.json (frames +
// commands), crates/engine/tests/golden_r2/*.json, crates/engine/tests/
// golden_bin/*.hex + golden_r2/*.bin.hex.
//
// Rules: never throws on bad input (returns null / drops bad entries);
// unknown `type` tags parse leniently into `{ kind: 'unknown', eventType }`
// so R2 frames from a newer engine never break an older UI.

import type { ParamValue } from './types.ts';

// ---------- small validators (total; no exceptions) ----------

function isRec(v: unknown): v is Record<string, unknown> {
  return typeof v === 'object' && v !== null && !Array.isArray(v);
}

function num(v: unknown, fallback = 0): number {
  return typeof v === 'number' && Number.isFinite(v) ? v : fallback;
}

function int(v: unknown, fallback: number | null = null): number | null {
  return typeof v === 'number' && Number.isInteger(v) ? v : fallback;
}

function str(v: unknown, fallback = ''): string {
  return typeof v === 'string' ? v : fallback;
}

function bool(v: unknown, fallback = false): boolean {
  return typeof v === 'boolean' ? v : fallback;
}

/** Wire param value coercion (same rule as lib/backend.ts: non-finite -> 0). */
export function normParamValue(raw: unknown): ParamValue {
  if (typeof raw === 'string' || typeof raw === 'boolean') return raw;
  if (typeof raw === 'number' && Number.isFinite(raw)) return raw;
  return 0;
}

function strArray(v: unknown): string[] {
  if (!Array.isArray(v)) return [];
  return v.filter((s): s is string => typeof s === 'string');
}

// ---------- event shapes ----------

export interface StateStripEvt {
  ch: number;
  name: string;
  fader: number;
  muted: boolean;
}
export interface CapsParamEvt {
  id: string;
  kind: 'float' | 'bool' | 'enum' | 'text';
  scope: string;
  scopes: string[];
  min: number | null;
  max: number | null;
  unit: string;
  default: ParamValue;
  label: string;
}
export interface TopologyEvt {
  key: string;
  kind: string;
  label: string;
  first: number;
  count: number;
  name_prefix: string;
  color_default: number;
  params_scope: string[];
}
export interface FxParamCapsEvt {
  id: string;
  label: string;
  unit: string;
  min: number;
  max: number;
  default: number;
  scale: string;
}
export interface FxTypeCapsEvt {
  id: string;
  label: string;
  params: FxParamCapsEvt[];
}

export type EngineEvent =
  | { kind: 'you'; id: string }
  | { kind: 'state'; driver: string; strips: StateStripEvt[]; main_fader: number | null; by?: string }
  | { kind: 'param'; ch: number; field: string; value: ParamValue; by?: string }
  | { kind: 'params'; ch: number; values: Record<string, ParamValue> }
  | {
      kind: 'caps';
      driver: string;
      params: CapsParamEvt[];
      unsupported: string[];
      topology: TopologyEvt[];
      meter_taps: string[];
      fx: { slots: number; types: FxTypeCapsEvt[] };
      routing: { groups: Array<{ key: string; label: string; count: number }>; per_strip: boolean; supported: boolean };
      groups: { buses: number; subgroups: number; mute_groups: number; mg_assign: boolean; sub_assign: boolean };
      files: { supported: boolean };
    }
  | { kind: 'meters'; levels: number[] }
  | { kind: 'spectrum'; ch: number; bins: number[] }
  | { kind: 'gr'; ch: number; gate: number; comp: number }
  | { kind: 'taps'; tap: string; levels: number[] }
  | { kind: 'bus'; bus: number; fader: number; muted: boolean }
  | { kind: 'subgroup'; group: number; fader: number; muted: boolean }
  | { kind: 'subgroup_assign'; ch: number; group: number; member: boolean }
  | { kind: 'mute_group'; group: number; muted: boolean }
  | { kind: 'mute_group_assign'; ch: number; group: number; member: boolean }
  | { kind: 'main'; fader: number | null; muted: boolean | null }
  | { kind: 'solo'; ch: number; soloed: boolean }
  | { kind: 'file'; filename: string; size: number; file_type: number; data: string | null }
  | { kind: 'shows'; shows: Array<{ name: string; scenes: Array<{ name: string; main_fader: number | null; strips: StateStripEvt[] }> }> }
  | { kind: 'locks'; holds: Record<number, string> }
  | { kind: 'clients'; count: number; writers: string[] }
  | { kind: 'error'; message: string; locked_by?: string; ch?: number }
  | { kind: 'recalled'; overrode_locks: number[] }
  | { kind: 'hello'; binary_meters: boolean; taps: boolean }
  | {
      kind: 'stats';
      meters_sent: number;
      meters_dropped: number;
      gr_sent: number;
      gr_dropped: number;
      spectrum_sent: number;
      spectrum_dropped: number;
      queue_depth: number;
      binary: boolean;
    }
  | {
      kind: 'devices';
      list: Array<{ id: string; name: string; model: string; ip: string; kind: string; state: string; fw: string | null; via: string | null }>;
      scanned_at: number;
    }
  | { kind: 'link'; state: string; name: string; ip: string; fs: number; latency_ms: number }
  | { kind: 'info'; model: string; firmware: string; sample_rate: number; console_name: string; driver: string }
  | { kind: 'scenes'; scenes: Array<{ number: number; name: string; current: boolean; saved_at: number }> }
  | {
      kind: 'fx';
      slots: Array<{ slot: number; type: string; on: boolean; bypass: boolean; ret_label: string; level: number; params: Record<string, number> }>;
    }
  | { kind: 'routing'; patches: Array<{ strip: number; group: string; index: number; label: string }> }
  | { kind: 'offline_diff'; changes: Array<{ strip: number; id: string; from: ParamValue; to: ParamValue }>; count: number }
  | { kind: 'unknown'; eventType: string };

function normStrip(s: unknown, i: number): StateStripEvt {
  const r = isRec(s) ? s : {};
  return {
    ch: int(r['ch']) ?? i,
    name: str(r['name'], 'CH' + String((int(r['ch']) ?? i) + 1).padStart(2, '0')),
    fader: Math.min(1, Math.max(0, num(r['fader'], 0.75))),
    muted: bool(r['muted'] ?? r['mute'], false),
  };
}

function normCapsParam(p: unknown): CapsParamEvt | null {
  if (!isRec(p)) return null;
  if (typeof p['id'] !== 'string' || !p['id']) return null;
  const kind = p['kind'];
  if (kind !== 'float' && kind !== 'bool' && kind !== 'enum' && kind !== 'text') return null;
  const mm = (v: unknown): number | null =>
    typeof v === 'number' && Number.isFinite(v) ? v : null;
  const raw = p['default'];
  const def: ParamValue =
    typeof raw === 'string' || typeof raw === 'boolean'
      ? raw
      : typeof raw === 'number' && Number.isFinite(raw)
        ? raw
        : kind === 'bool'
          ? false
          : kind === 'text'
            ? ''
            : 0;
  const scopesRaw = p['scopes'];
  const scopes = Array.isArray(scopesRaw)
    ? scopesRaw.filter((s): s is string => typeof s === 'string')
    : [];
  return {
    id: p['id'],
    kind,
    scope: str(p['scope'], 'channel'),
    scopes,
    min: mm(p['min']),
    max: mm(p['max']),
    unit: str(p['unit']),
    default: def,
    label: str(p['label'], p['id']),
  };
}

function normTopology(t: unknown): TopologyEvt | null {
  if (!isRec(t)) return null;
  if (typeof t['key'] !== 'string' || !t['key']) return null;
  if (typeof t['kind'] !== 'string' || !t['kind']) return null;
  const first = int(t['first']);
  const count = int(t['count']);
  if (first === null || count === null || first < 0 || count < 0) return null;
  return {
    key: t['key'],
    kind: t['kind'],
    label: str(t['label'], t['key']),
    first,
    count,
    name_prefix: str(t['name_prefix'], t['key']),
    color_default: int(t['color_default']) ?? 0,
    params_scope: strArray(t['params_scope']),
  };
}

/**
 * Parse one wire JSON text frame. Returns null for unparseable input
 * (invalid JSON / non-object / missing `type`); unknown `type` tags become
 * `{ kind: 'unknown', eventType }` — never throws.
 */
export function parseEvent(text: string): EngineEvent | null {
  let m: unknown;
  try {
    m = JSON.parse(text);
  } catch {
    return null;
  }
  if (!isRec(m)) return null;
  // Commands share the `type` key (fx_set_type's fx `type`): anything with a
  // `cmd` tag is a command, never an event.
  if (typeof m['cmd'] === 'string') return null;
  const t = m['type'];
  if (typeof t !== 'string' || !t) return null;
  switch (t) {
    case 'you':
      return typeof m['id'] === 'string' && m['id'] ? { kind: 'you', id: m['id'] } : null;
    case 'state': {
      if (!Array.isArray(m['strips'])) return null;
      const mf = m['main_fader'];
      return {
        kind: 'state',
        driver: str(m['driver'], ''),
        strips: m['strips'].map((s, i) => normStrip(s, i)),
        main_fader: typeof mf === 'number' && Number.isFinite(mf) ? Math.min(1, Math.max(0, mf)) : null,
        ...(typeof m['by'] === 'string' ? { by: m['by'] } : {}),
      };
    }
    case 'param': {
      const ch = int(m['ch']);
      if (ch === null || ch < 0 || typeof m['field'] !== 'string' || !m['field']) return null;
      return {
        kind: 'param',
        ch,
        field: m['field'],
        value: normParamValue(m['value']),
        ...(typeof m['by'] === 'string' ? { by: m['by'] } : {}),
      };
    }
    case 'params': {
      const ch = int(m['ch']);
      if (ch === null || ch < 0 || !isRec(m['values'])) return null;
      const values: Record<string, ParamValue> = {};
      for (const [k, v] of Object.entries(m['values'])) {
        if (k) values[k] = normParamValue(v);
      }
      return { kind: 'params', ch, values };
    }
    case 'caps': {
      if (!Array.isArray(m['params'])) return null;
      const params: CapsParamEvt[] = [];
      for (const p of m['params']) {
        const d = normCapsParam(p);
        if (d) params.push(d);
      }
      const fxRaw = isRec(m['fx']) ? m['fx'] : {};
      const typesRaw = Array.isArray(fxRaw['types']) ? fxRaw['types'] : [];
      const types: FxTypeCapsEvt[] = [];
      for (const ft of typesRaw) {
        if (!isRec(ft) || typeof ft['id'] !== 'string') continue;
        const ps: FxParamCapsEvt[] = [];
        if (Array.isArray(ft['params'])) {
          for (const pp of ft['params']) {
            if (!isRec(pp) || typeof pp['id'] !== 'string') continue;
            ps.push({
              id: pp['id'],
              label: str(pp['label'], pp['id']),
              unit: str(pp['unit']),
              min: num(pp['min']),
              max: num(pp['max']),
              default: num(pp['default']),
              scale: str(pp['scale']),
            });
          }
        }
        types.push({ id: ft['id'], label: str(ft['label'], ft['id']), params: ps });
      }
      const rtRaw = isRec(m['routing']) ? m['routing'] : {};
      const groups: Array<{ key: string; label: string; count: number }> = [];
      if (Array.isArray(rtRaw['groups'])) {
        for (const g of rtRaw['groups']) {
          if (!isRec(g) || typeof g['key'] !== 'string') continue;
          groups.push({ key: g['key'], label: str(g['label'], g['key']), count: int(g['count']) ?? 0 });
        }
      }
      const grpRaw = isRec(m['groups']) ? m['groups'] : {};
      const filesRaw = isRec(m['files']) ? m['files'] : {};
      const topo: TopologyEvt[] = [];
      if (Array.isArray(m['topology'])) {
        for (const f of m['topology']) {
          const fam = normTopology(f);
          if (fam) topo.push(fam);
        }
      }
      return {
        kind: 'caps',
        driver: str(m['driver'], ''),
        params,
        unsupported: strArray(m['unsupported']),
        topology: topo,
        meter_taps: strArray(m['meter_taps']),
        fx: { slots: int(fxRaw['slots']) ?? 0, types },
        routing: {
          groups,
          per_strip: bool(rtRaw['per_strip'], false),
          supported: bool(rtRaw['supported'], false),
        },
        groups: {
          buses: int(grpRaw['buses']) ?? 0,
          subgroups: int(grpRaw['subgroups']) ?? 0,
          mute_groups: int(grpRaw['mute_groups']) ?? 0,
          mg_assign: grpRaw['mg_assign'] === undefined ? true : bool(grpRaw['mg_assign'], true),
          sub_assign: grpRaw['sub_assign'] === undefined ? true : bool(grpRaw['sub_assign'], true),
        },
        files: { supported: bool(filesRaw['supported'], false) },
      };
    }
    case 'meters':
      if (!Array.isArray(m['levels'])) return null;
      return { kind: 'meters', levels: m['levels'].map((v: unknown) => num(v)) };
    case 'spectrum': {
      const ch = int(m['ch']);
      if (ch === null || !Array.isArray(m['bins'])) return null;
      return { kind: 'spectrum', ch, bins: m['bins'].map((v: unknown) => num(v)) };
    }
    case 'gr': {
      const ch = int(m['ch']);
      if (ch === null) return null;
      return { kind: 'gr', ch, gate: num(m['gate']), comp: num(m['comp']) };
    }
    case 'taps': {
      if (typeof m['tap'] !== 'string' || !Array.isArray(m['levels'])) return null;
      return { kind: 'taps', tap: m['tap'], levels: m['levels'].map((v: unknown) => num(v)) };
    }
    case 'shows': {
      if (!Array.isArray(m['shows'])) return null;
      const out: Array<{ name: string; scenes: Array<{ name: string; main_fader: number | null; strips: StateStripEvt[] }> }> = [];
      for (const s of m['shows']) {
        if (!isRec(s) || typeof s['name'] !== 'string' || !Array.isArray(s['scenes'])) continue;
        const scenes: Array<{ name: string; main_fader: number | null; strips: StateStripEvt[] }> = [];
        for (const sc of s['scenes']) {
          if (!isRec(sc) || typeof sc['name'] !== 'string' || !Array.isArray(sc['strips'])) continue;
          const mf = sc['main_fader'];
          scenes.push({
            name: sc['name'],
            main_fader: typeof mf === 'number' && Number.isFinite(mf) ? mf : null,
            strips: sc['strips'].map((x, i) => normStrip(x, i)),
          });
        }
        out.push({ name: s['name'], scenes });
      }
      return { kind: 'shows', shows: out };
    }
    case 'locks': {
      if (!isRec(m['holds'])) return null;
      const holds: Record<number, string> = {};
      for (const [k, v] of Object.entries(m['holds'])) {
        const ch = Number(k);
        if (Number.isInteger(ch) && ch >= 0 && typeof v === 'string' && v) holds[ch] = v;
      }
      return { kind: 'locks', holds };
    }
    case 'clients':
      return {
        kind: 'clients',
        count: int(m['count']) ?? 0,
        writers: strArray(m['writers']),
      };
    case 'error':
      return {
        kind: 'error',
        message: str(m['message'], 'request denied'),
        ...(typeof m['locked_by'] === 'string' && m['locked_by'] ? { locked_by: m['locked_by'] } : {}),
        ...(int(m['ch']) !== null ? { ch: int(m['ch']) as number } : {}),
      };
    case 'recalled':
      return {
        kind: 'recalled',
        overrode_locks: Array.isArray(m['overrode_locks'])
          ? m['overrode_locks'].filter((v): v is number => typeof v === 'number' && Number.isFinite(v))
          : [],
      };
    case 'hello':
      return { kind: 'hello', binary_meters: bool(m['binary_meters'], false), taps: bool(m['taps'], false) };
    case 'stats':
      return {
        kind: 'stats',
        meters_sent: int(m['meters_sent']) ?? 0,
        meters_dropped: int(m['meters_dropped']) ?? 0,
        gr_sent: int(m['gr_sent']) ?? 0,
        gr_dropped: int(m['gr_dropped']) ?? 0,
        spectrum_sent: int(m['spectrum_sent']) ?? 0,
        spectrum_dropped: int(m['spectrum_dropped']) ?? 0,
        queue_depth: int(m['queue_depth']) ?? 0,
        binary: bool(m['binary'], false),
      };
    case 'devices': {
      if (!Array.isArray(m['list'])) return null;
      const list: Array<{ id: string; name: string; model: string; ip: string; kind: string; state: string; fw: string | null; via: string | null }> = [];
      for (const d of m['list']) {
        if (!isRec(d) || typeof d['id'] !== 'string') continue;
        list.push({
          id: d['id'],
          name: str(d['name'], d['id']),
          model: str(d['model']),
          ip: str(d['ip']),
          kind: str(d['kind']),
          state: str(d['state']),
          fw: typeof d['fw'] === 'string' ? d['fw'] : null,
          via: typeof d['via'] === 'string' ? d['via'] : null,
        });
      }
      return { kind: 'devices', list, scanned_at: int(m['scanned_at']) ?? 0 };
    }
    case 'link':
      return {
        kind: 'link',
        state: str(m['state'], 'disconnected'),
        name: str(m['name']),
        ip: str(m['ip']),
        fs: int(m['fs']) ?? 0,
        latency_ms: num(m['latency_ms']),
      };
    case 'info':
      return {
        kind: 'info',
        model: str(m['model']),
        firmware: str(m['firmware']),
        sample_rate: int(m['sample_rate']) ?? 0,
        console_name: str(m['console_name']),
        driver: str(m['driver'], ''),
      };
    case 'scenes': {
      if (!Array.isArray(m['scenes'])) return null;
      const scenes: Array<{ number: number; name: string; current: boolean; saved_at: number }> = [];
      for (const s of m['scenes']) {
        if (!isRec(s) || int(s['number']) === null || typeof s['name'] !== 'string') continue;
        scenes.push({
          number: int(s['number']) as number,
          name: s['name'],
          current: bool(s['current'], false),
          saved_at: int(s['saved_at']) ?? 0,
        });
      }
      return { kind: 'scenes', scenes };
    }
    case 'fx': {
      if (!Array.isArray(m['slots'])) return null;
      const slots: Array<{ slot: number; type: string; on: boolean; bypass: boolean; ret_label: string; level: number; params: Record<string, number> }> = [];
      for (const s of m['slots']) {
        if (!isRec(s) || int(s['slot']) === null || typeof s['type'] !== 'string') continue;
        const params: Record<string, number> = {};
        if (isRec(s['params'])) {
          for (const [k, v] of Object.entries(s['params'])) {
            if (k) params[k] = num(v);
          }
        }
        slots.push({
          slot: int(s['slot']) as number,
          type: s['type'],
          on: bool(s['on'], true),
          bypass: bool(s['bypass'], false),
          ret_label: str(s['ret_label']),
          level: num(s['level']),
          params,
        });
      }
      return { kind: 'fx', slots };
    }
    case 'routing': {
      if (!Array.isArray(m['patches'])) return null;
      const patches: Array<{ strip: number; group: string; index: number; label: string }> = [];
      for (const p of m['patches']) {
        if (!isRec(p) || int(p['strip']) === null || typeof p['group'] !== 'string') continue;
        patches.push({
          strip: int(p['strip']) as number,
          group: p['group'],
          index: int(p['index']) ?? 0,
          label: str(p['label']),
        });
      }
      return { kind: 'routing', patches };
    }
    case 'offline_diff': {
      if (!Array.isArray(m['changes'])) return null;
      const changes: Array<{ strip: number; id: string; from: ParamValue; to: ParamValue }> = [];
      for (const c of m['changes']) {
        if (!isRec(c) || int(c['strip']) === null || typeof c['id'] !== 'string') continue;
        changes.push({
          strip: int(c['strip']) as number,
          id: c['id'],
          from: normParamValue(c['from']),
          to: normParamValue(c['to']),
        });
      }
      return { kind: 'offline_diff', changes, count: int(m['count']) ?? changes.length };
    }
    case 'bus': {
      const bus = int(m['bus']);
      if (bus === null || bus < 0) return null;
      return { kind: 'bus', bus, fader: num(m['fader']), muted: bool(m['muted'], false) };
    }
    case 'subgroup': {
      const group = int(m['group']);
      if (group === null || group < 0) return null;
      return { kind: 'subgroup', group, fader: num(m['fader']), muted: bool(m['muted'], false) };
    }
    case 'subgroup_assign': {
      const ch = int(m['ch']);
      const group = int(m['group']);
      if (ch === null || ch < 0 || group === null || group < 0) return null;
      return { kind: 'subgroup_assign', ch, group, member: bool(m['member'], false) };
    }
    case 'mute_group': {
      const group = int(m['group']);
      if (group === null || group < 0) return null;
      return { kind: 'mute_group', group, muted: bool(m['muted'], false) };
    }
    case 'mute_group_assign': {
      const ch = int(m['ch']);
      const group = int(m['group']);
      if (ch === null || ch < 0 || group === null || group < 0) return null;
      return { kind: 'mute_group_assign', ch, group, member: bool(m['member'], false) };
    }
    case 'main': {
      const f = m['fader'];
      const mu = m['muted'];
      return {
        kind: 'main',
        fader: typeof f === 'number' && Number.isFinite(f) ? f : null,
        muted: typeof mu === 'boolean' ? mu : null,
      };
    }
    case 'solo': {
      const ch = int(m['ch']);
      if (ch === null || ch < 0) return null;
      return { kind: 'solo', ch, soloed: bool(m['soloed'], false) };
    }
    case 'file': {
      if (typeof m['filename'] !== 'string' || !m['filename']) return null;
      return {
        kind: 'file',
        filename: m['filename'],
        size: int(m['size']) ?? 0,
        file_type: int(m['file_type']) ?? 0,
        data: typeof m['data'] === 'string' ? m['data'] : null,
      };
    }
    default:
      // Lenient: newer-engine frames (or future R2 types) never break parsing.
      return { kind: 'unknown', eventType: t };
  }
}

// ---------- binary frames (MR1/R2: README layout + protocol.rs constants) ----------

export const BIN_TYPE_METERS = 1;
export const BIN_TYPE_GR = 2;
export const BIN_TYPE_TAPS = 3;
export const BIN_VERSION = 1;
export const BIN_HEADER_LEN = 12;
export const GR_DB_MAX = 63.75;
export const TAPS_GR_DB_FULLSCALE = 24;

export type BinaryFrame =
  | { kind: 'meters'; seq: number; tMs: number; levels: number[] }
  | { kind: 'gr'; seq: number; tMs: number; ch: number; gate: number; comp: number }
  | { kind: 'taps'; seq: number; tMs: number; tap: string; levels: number[] };

const TAP_NAMES = ['in', 'post', 'gr_gate', 'gr_comp'] as const;

function u16ToLevel(q: number): number {
  return q / 65535;
}

/**
 * Decode one WS BINARY frame. Returns null for short/unknown frames —
 * never throws. Views the buffer in place (levels arrays are fresh; the
 * EngineWorld copies them into its preallocated typed arrays).
 */
export function parseBinary(data: ArrayBuffer | Uint8Array): BinaryFrame | null {
  const bytes = data instanceof Uint8Array ? data : new Uint8Array(data);
  if (bytes.length < BIN_HEADER_LEN) return null;
  const type = bytes[0]!;
  const version = bytes[1]!;
  if (version !== BIN_VERSION) return null;
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.length);
  const n = view.getUint16(2, true);
  const seq = view.getUint32(4, true);
  const tMs = view.getUint32(8, true);
  if (type === BIN_TYPE_METERS) {
    if (bytes.length < BIN_HEADER_LEN + n * 2) return null;
    const levels: number[] = new Array(n);
    for (let i = 0; i < n; i++) levels[i] = u16ToLevel(view.getUint16(BIN_HEADER_LEN + i * 2, true));
    return { kind: 'meters', seq, tMs, levels };
  }
  if (type === BIN_TYPE_GR) {
    // One entry per frame: n=1, then (u8 ch, u8 gate dBx4, u8 comp dBx4).
    if (n !== 1 || bytes.length < BIN_HEADER_LEN + 3) return null;
    return {
      kind: 'gr',
      seq,
      tMs,
      ch: bytes[BIN_HEADER_LEN]!,
      gate: bytes[BIN_HEADER_LEN + 1]! / 4,
      comp: bytes[BIN_HEADER_LEN + 2]! / 4,
    };
  }
  if (type === BIN_TYPE_TAPS) {
    // [u8 tap id] then n x u16 (GR taps scaled by 24 dB full-scale).
    if (bytes.length < BIN_HEADER_LEN + 1 + n * 2) return null;
    const tapId = bytes[BIN_HEADER_LEN]!;
    const tap = tapId < TAP_NAMES.length ? TAP_NAMES[tapId]! : 'unknown-' + tapId;
    const gr = tap === 'gr_gate' || tap === 'gr_comp';
    const levels: number[] = new Array(n);
    for (let i = 0; i < n; i++) {
      const q = u16ToLevel(view.getUint16(BIN_HEADER_LEN + 1 + i * 2, true));
      levels[i] = gr ? q * TAPS_GR_DB_FULLSCALE : q;
    }
    return { kind: 'taps', seq, tMs, tap, levels };
  }
  return null;
}

// ---------- command builders (Cmd in; JSON strings for the socket) ----------

export type EngineCmd = Record<string, unknown>;

function cmd(o: EngineCmd): string {
  return JSON.stringify(o);
}

export const cmdConnect = (driver = 'sim', addr?: string): string =>
  cmd(addr === undefined ? { cmd: 'connect', driver } : { cmd: 'connect', driver, addr });
export const cmdFader = (ch: number, value: number): string => cmd({ cmd: 'fader', ch, value });
export const cmdMainFader = (value: number): string => cmd({ cmd: 'main_fader', value });
export const cmdMute = (ch: number, muted: boolean): string => cmd({ cmd: 'mute', ch, muted });
export const cmdMainMute = (muted: boolean): string => cmd({ cmd: 'main_mute', muted });
export const cmdSolo = (ch: number, soloed: boolean): string => cmd({ cmd: 'solo', ch, soloed });
export const cmdParam = (ch: number, field: string, value: ParamValue): string =>
  cmd({ cmd: 'param', ch, field, value });
export const cmdFocus = (chs: number[]): string =>
  cmd(chs.length === 1 ? { cmd: 'focus', ch: chs[0] } : { cmd: 'focus', chs });
export const cmdLock = (ch: number): string => cmd({ cmd: 'lock', ch });
export const cmdUnlock = (ch: number): string => cmd({ cmd: 'unlock', ch });
export const cmdHello = (binary_meters?: boolean, taps?: boolean): string => {
  const o: EngineCmd = { cmd: 'hello' };
  if (binary_meters !== undefined) o['binary_meters'] = binary_meters;
  if (taps !== undefined) o['taps'] = taps;
  return cmd(o);
};
export const cmdListShows = (): string => cmd({ cmd: 'list_shows' });
export const cmdCaptureScene = (show: string, name: string): string => cmd({ cmd: 'capture_scene', show, name });
export const cmdRecallScene = (show: string, name: string, fade_ms?: number | null): string =>
  cmd({ cmd: 'recall_scene', show, name, fade_ms: fade_ms === undefined ? null : fade_ms });
export const cmdDeleteScene = (show: string, name: string): string => cmd({ cmd: 'delete_scene', show, name });
export const cmdScan = (): string => cmd({ cmd: 'scan' });
export const cmdProbeDesk = (addr: string): string => cmd({ cmd: 'probe', addr });
export const cmdConnectDevice = (id: string): string => cmd({ cmd: 'connect_device', id });
export const cmdDisconnect = (): string => cmd({ cmd: 'disconnect' });
export const cmdInfo = (): string => cmd({ cmd: 'info' });
export const cmdSceneSave = (n: number, name: string): string => cmd({ cmd: 'scene_save', n, name });
export interface RecallScopeCmd { strips?: number[]; groups?: string[]; globals?: string[] }
export const cmdSceneRecall = (n: number, fade_ms?: number, scope?: RecallScopeCmd | null): string => {
  const o: Record<string, unknown> = { cmd: 'scene_recall', n };
  if (fade_ms !== undefined) o['fade_ms'] = fade_ms;
  if (scope !== undefined && scope !== null) o['scope'] = scope;
  return cmd(o);
};
export const cmdSceneRename = (n: number, name: string): string => cmd({ cmd: 'scene_rename', n, name });
export const cmdSceneDelete = (n: number): string => cmd({ cmd: 'scene_delete', n });
export const cmdSceneClear = (): string => cmd({ cmd: 'scene_clear' });
export const cmdFxSetType = (slot: number, type: string): string => cmd({ cmd: 'fx_set_type', slot, type });
export const cmdFxParam = (slot: number, id: string, value: number): string => cmd({ cmd: 'fx_param', slot, id, value });
export const cmdFxBypass = (slot: number, bypass: boolean): string => cmd({ cmd: 'fx_bypass', slot, bypass });
export const cmdFxOn = (slot: number, on: boolean): string => cmd({ cmd: 'fx_on', slot, on });
export const cmdPatchSet = (strip: number, group: string, index: number): string =>
  cmd({ cmd: 'patch_set', strip, group, index });
export const cmdPatchClear = (strip: number): string => cmd({ cmd: 'patch_clear', strip });
export const cmdOffline = (model: string): string => cmd({ cmd: 'offline', model });
export const cmdOfflinePush = (confirm: boolean): string => cmd({ cmd: 'offline_push', confirm });
export const cmdOfflineDiscard = (): string => cmd({ cmd: 'offline_discard' });
export const cmdBusFader = (bus: number, value: number): string => cmd({ cmd: 'bus_fader', bus, value });
export const cmdBusMute = (bus: number, muted: boolean): string => cmd({ cmd: 'bus_mute', bus, muted });
export const cmdSubgroupFader = (group: number, value: number): string => cmd({ cmd: 'subgroup_fader', group, value });
export const cmdSubgroupMute = (group: number, muted: boolean): string => cmd({ cmd: 'subgroup_mute', group, muted });
export const cmdSubgroupAssign = (ch: number, group: number, member: boolean): string =>
  cmd({ cmd: 'subgroup_assign', ch, group, member });
export const cmdMuteGroup = (group: number, muted: boolean): string => cmd({ cmd: 'mute_group', group, muted });
export const cmdMuteGroupAssign = (ch: number, group: number, member: boolean): string =>
  cmd({ cmd: 'mute_group_assign', ch, group, member });
export const cmdFileDownload = (filename: string): string => cmd({ cmd: 'file_download', filename });
export const cmdFileUpload = (filename: string, data: string, file_type: number): string =>
  cmd({ cmd: 'file_upload', filename, data, file_type });
/** Adapter-level diagnostics (serve only; the engine answers `unknown cmd`). */
export const cmdStats = (): string => cmd({ cmd: 'stats' });

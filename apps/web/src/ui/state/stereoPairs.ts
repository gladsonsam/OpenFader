// Linked-fader pairing: where the app KNOWS two strips are a stereo pair,
// moving one fader moves the other — computed locally, no WS round-trip,
// no re-read. The link IS the state: pairing fires only on confirmed link
// knowledge, never on name resemblance alone.
//
// Two authorities, in priority order (consulted per gesture by uiStore):
//  1. Live engine (online + capsReady): the per-strip `link` leaf. The
//     strip under the finger must report link===true AND an index-adjacent
//     same-base L/R mate (sim: "OH L"/"OH R") must also report link===true.
//     All reads are cached (supports/param touch no socket).
//  2. Fixture mode (no live world): the fixture src convention. FX returns
//     carry `FX kL`/`FX kR` src labels (fx1/fx2 share FX 1, ...), the only
//     stereo-pair signal in the design fixtures. Input channels ("Ch N")
//     are mono: no pairing.
//
// Honest scoping (NOT faked): on xair the per-channel `link` leaf is
// unsupported — stereo link there is the global /config/chlink switch,
// which no driver/engine path surfaces (proto-xair/src/look.rs documents
// the missing leaf; `link` is excluded from XAIR_CAPABILITIES in
// proto-xair/src/lib.rs; the engine never polls /config/chlink/*). So on
// xair livePartnerId returns null and single-strip moves stay single.
// Wiring that read path (driver chlink query -> engine global -> caps +
// poller -> per-strip derivation) is the follow-up; see report.
//
// Only fader + mute pair (the desk-family stereo-link gang set the owner
// asked for). Pan/sends never pair here.

import { bridgeStrip } from './engineBridge.ts';
import type { Channel } from '../fixtures/designData.ts';
import type { World as EngineWorldIface } from '../world/types.ts';

/** Split a trailing " L" / " R" stereo suffix ("OH L" -> { base: "OH", side: "L" }). Null without one. */
export function stereoBase(name: string): { base: string; side: 'L' | 'R' } | null {
  if (name.endsWith(' L')) return { base: name.slice(0, -2), side: 'L' };
  if (name.endsWith(' R')) return { base: name.slice(0, -2), side: 'R' };
  return null;
}

/** Fixture stereo partner from the src convention (`FX kL` <-> `FX kR`,
 *  adjacent fx(2k-1)/fx(2k)). Null for mono strips and unknown ids. */
export function fixturePartnerId(chans: Record<string, Channel>, id: string): string | null {
  const m = /^fx(\d+)$/.exec(id);
  if (!m) return null;
  const n = parseInt(m[1]!, 10);
  if (!Number.isInteger(n) || n < 1) return null;
  const mateN = n % 2 === 1 ? n + 1 : n - 1;
  if (mateN < 1) return null;
  const mateId = 'fx' + mateN;
  const a = chans[id];
  const b = chans[mateId];
  if (!a || !b || typeof a.src !== 'string' || typeof b.src !== 'string') return null;
  const pa = /^(.*)([LR])$/.exec(a.src.replace(/\s+/g, ''));
  const pb = /^(.*)([LR])$/.exec(b.src.replace(/\s+/g, ''));
  if (!pa || !pb) return null;
  if (pa[1] !== pb[1] || pa[2] === pb[2]) return null;
  return mateId;
}

/** Fixture id currently mapped to a live engine strip (inverse of
 * bridgeStrip; gesture-rate scan, no reads). Null when unmapped. */
function fixtureIdForEngineCh(
  w: EngineWorldIface,
  chans: Record<string, Channel>,
  ch: number,
): string | null {
  for (const id of Object.keys(chans)) {
    if (bridgeStrip(w, id) === ch) return id;
  }
  return null;
}

/** Live stereo partner: engine strip for `id` reports link===true with an
 * index-adjacent same-base L/R mate that also reports link===true.
 * Cached param/caps reads only — never touches the socket. Null offline,
 * unsupported (xair), unlinked, or unmapped. */
export function livePartnerId(
  w: EngineWorldIface,
  chans: Record<string, Channel>,
  id: string,
): string | null {
  if (id === 'mainlr') return null;
  const ch = bridgeStrip(w, id);
  if (ch === null) return null;
  if (!w.supports('link', ch)) return null;
  if (w.param(ch, 'link') !== true) return null;
  const name = w.param(ch, 'name');
  if (typeof name !== 'string') return null;
  const sb = stereoBase(name);
  if (!sb || !sb.base) return null;
  const want = sb.base + (sb.side === 'L' ? ' R' : ' L');
  const kind = w.kindOf(ch);
  for (const mate of [ch - 1, ch + 1]) {
    if (mate < 0 || mate >= w.stripCount) continue;
    if (kind !== null && w.kindOf(mate) !== kind) continue;
    if (w.param(mate, 'link') !== true) continue;
    if (w.param(mate, 'name') !== want) continue;
    return fixtureIdForEngineCh(w, chans, mate);
  }
  return null;
}

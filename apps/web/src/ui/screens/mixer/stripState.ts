// Echo-confirmed per-strip status for the glanceable `strip-state-icons`
// row (mixer strips incl. master + channel-detail master). Pure over the
// live EngineWorld: MixerScreen/ChannelScreen call it with the live world +
// engine strip and bind the result straight through (the same DeskGroups
// object the group badges read, extended with preamp leaves).
//
// Every leaf is confirmed-only — null hides the glyph, never faked:
// `supports()` gates scope (phantom on a bus/main strip reports nothing,
// link on xair is the global chlink switch, not a per-strip leaf),
// `status() === 'known'` gates the echo (optimistic writes stay hidden
// until the desk confirms them).

import type { EngineWorld } from '../../world/engineWorld/engineWorld.ts';
import type { DeskGroups } from './mixerVm.ts';

/** Confirmed per-strip status (null = hide everything: offline, unmapped,
 *  unsupported, or nothing echoed yet — never faked). */
export function stripStateOf(w: EngineWorld, ch: number): DeskGroups | null {
  const caps = w.groupCaps;
  const mg: number[] = [];
  const mgMuted: Array<boolean | null> = [];
  for (let g = 0; g < caps.muteGroups; g++) {
    if (w.muteGroupAssigned(ch, g) === true) {
      mg.push(g);
      mgMuted.push(w.muteGroupMaster(g));
    }
  }
  const sg: number[] = [];
  const sgMuted: Array<boolean | null> = [];
  for (let g = 0; g < caps.subgroups; g++) {
    if (w.subgroupAssigned(ch, g) === true) {
      sg.push(g);
      sgMuted.push(w.subgroupMaster(g)?.muted ?? null);
    }
  }
  // `link` is a real per-strip leaf on sim (unsupported on xair, where
  // stereo link is the global /config/chlink switch): null hides the glyph.
  // Status-gated like the rest: an unconfirmed value stays hidden.
  let link: boolean | null = null;
  if (w.supports('link', ch) && w.status(ch, 'link') === 'known') {
    const lv = w.param(ch, 'link');
    if (typeof lv === 'boolean') link = lv;
  }
  const confirmed = (id: string): boolean | null => {
    if (!w.supports(id, ch)) return null;
    const v = w.param(ch, id);
    if (typeof v !== 'boolean') return null;
    return w.status(ch, id) === 'known' ? v : null;
  };
  const phantom = confirmed('phantom');
  const polarity = confirmed('polarity');
  const delayOn = confirmed('delay_on');
  if (
    mg.length === 0 && sg.length === 0 && link === null &&
    phantom === null && polarity === null && delayOn === null
  ) return null;
  return { mg, sg, mgMuted, sgMuted, link, phantom, polarity, delayOn };
}

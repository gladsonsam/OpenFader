// uiLogic/mix: fader / send / pan / mute + drag math (lock-checked).
// Pure, rune-free logic split out of state/uiLogic.ts (one version of
// the logic lives here; uiLogic.ts re-exports it). DOM-free, node-testable.

import { clamp } from '../../fixtures/designData.ts';
import { CHAN_ICON_NAMES, type ChanIconName } from '../../icons.ts';
import { type World, withChannel } from './types.ts';

// ---------- fader / pan / mute (lock-checked, ports of move/onMute) ----------

export function dragFine(w: World): number {
  return w.state.fine ? 0.25 : 1;
}

/** Port of the fader branch of move(): drag pixels -> new 0..1 value. */
export function faderFromDrag(start: number, dyPx: number, trackH: number, fine: boolean): number {
  return clamp(start + (dyPx / trackH) * (fine ? 0.25 : 1), 0, 1);
}

export function setFader(w: World, id: string, v: number): World {
  if (w.state.locked) return w;
  return withChannel(w, id, c => { c.fader = clamp(v, 0, 1); });
}

export function setSend(w: World, id: string, k: number, v: number): World {
  if (w.state.locked) return w;
  return withChannel(w, id, c => {
    if (c.sends) c.sends[k] = Math.round(clamp(v, 0, 1) * 1000) / 1000;
  });
}

/** Port of the pan branch of move(). */
export function panFromDrag(start: number, dxPx: number, trackW: number, fine: boolean): number {
  let v = clamp(start + (dxPx / trackW) * 2 * (fine ? 0.25 : 1), -1, 1);
  if (Math.abs(v) < 0.03) v = 0;
  return Math.round(v * 100) / 100;
}

export function setPan(w: World, id: string, v: number): World {
  if (w.state.locked) return w;
  return withChannel(w, id, c => { if (c.pan != null) c.pan = Math.round(clamp(v, -1, 1) * 100) / 100; });
}

export function resetPan(w: World, id: string): World {
  if (w.state.locked) return w;
  return withChannel(w, id, c => { if (c.pan != null) c.pan = 0; });
}

export function toggleMute(w: World, id: string): World {
  if (!w.state.muteEnable || w.state.locked) return w;
  return withChannel(w, id, c => { c.mute = !c.mute; });
}

/** Solo flip, same gate as mute (muteEnable + lock). UI-only: the desk
 *  protocol has no solo counterpart, so nothing is sent to the relay. */
export function toggleSolo(w: World, id: string): World {
  if (!w.state.muteEnable || w.state.locked) return w;
  return withChannel(w, id, c => { c.solo = !c.solo; });
}

/** Rename a channel (blank names are ignored; lock-checked). */
export function setChannelName(w: World, id: string, name: string): World {
  if (w.state.locked) return w;
  const clean = name.trim().slice(0, 12);
  if (!clean) return w;
  return withChannel(w, id, c => { c.name = clean; });
}

/** Recolour a channel name bar (lock-checked). */
export function setChannelColor(w: World, id: string, color: string, text: string): World {
  if (w.state.locked) return w;
  if (!/^#[0-9a-fA-F]{6}$/.test(color) || !/^#[0-9a-fA-F]{6}$/.test(text)) return w;
  return withChannel(w, id, c => { c.color = color; c.text = text; });
}

/** Set a channel strip icon (UI-only, like solo; lock-checked). Null or
 * '' clears it; unknown names are ignored. */
export function setChannelIcon(w: World, id: string, icon: string | null): World {
  if (w.state.locked) return w;
  const clean = icon == null || icon === '' ? null : String(icon);
  if (clean !== null && !CHAN_ICON_NAMES.includes(clean as ChanIconName)) return w;
  return withChannel(w, id, c => { c.icon = clean; });
}
// ---------- sends ----------

export function toggleSendPre(w: World, id: string, k: number): World {
  if (w.state.locked) return w;
  return withChannel(w, id, c => { if (c.sendPre) c.sendPre[k] = !c.sendPre[k]; });
}

/** Port of the 'h' branch of move(): horizontal send drag -> 0..1. */
export function sendFromDrag(start: number, dxPx: number, trackW: number, fine: boolean): number {
  return clamp(start + (dxPx / trackW) * (fine ? 0.25 : 1), 0, 1);
}

// uiLogic/routing: routing patch selection + patch application.
// Pure, rune-free logic split out of state/uiLogic.ts (one version of
// the logic lives here; uiLogic.ts re-exports it). DOM-free, node-testable.

import { clamp } from '../../fixtures/designData.ts';
import { type RouteGroup, type World, withChannel, withState } from './types.ts';

// ---------- routing ----------

export function setPatchSel(w: World, id: string): World {
  return withState(w, { patchSel: id });
}

export function setPatchGroup(w: World, g: RouteGroup): World {
  return withState(w, { patchGroup: g });
}

export function setRouteBank(w: World, b: number): World {
  return withState(w, { routeBank: clamp(Math.round(b), 0, 2) });
}

/** Channel ids shown in the routing tile grid for a route bank. */
export function routeBankIds(routeBank: number): string[] {
  return Array.from({ length: 16 }, (_, k) => 'in' + (routeBank * 16 + k + 1));
}

/** Source tile labels for a patch group (32 sockets, `Lcl 01` style). */
export function srcTileLabels(patchGroup: RouteGroup): string[] {
  return Array.from({ length: 32 }, (_, k) => patchGroup + ' ' + String(k + 1).padStart(2, '0'));
}

/** Port of routing next(): patching advances to the next channel, wrapping. */
export function nextPatchSel(patchSel: string): { patchSel: string; routeBank: number } {
  const n = parseInt(patchSel.replace(/^\D+/, ''), 10);
  const nn = Number.isFinite(n) ? (n % 48) + 1 : 1;
  return { patchSel: 'in' + nn, routeBank: Math.floor((nn - 1) / 16) };
}

export function patchSrc(w: World, label: string): World {
  if (w.state.locked) return w;
  const id = w.state.patchSel;
  const patched = withChannel(w, id, c => { c.src = label; });
  return withState(patched, nextPatchSel(id));
}

export function unpatch(w: World): World {
  if (w.state.locked) return w;
  return withChannel(w, w.state.patchSel, c => { c.src = '—'; });
}

/** Matrix patch: set one channel's source without advancing the selection
 * (the grid stays put so the user can keep patching). Lock-checked. */
export function patchChannelSrc(w: World, id: string, label: string): World {
  if (w.state.locked) return w;
  return withChannel(w, id, c => { c.src = label; });
}

/** Matrix unpatch: clear one channel's source. Lock-checked. */
export function unpatchChannel(w: World, id: string): World {
  if (w.state.locked) return w;
  return withChannel(w, id, c => { c.src = '—'; });
}

/** Matrix cell tap: tapping the channel's current source unpatches it,
 * tapping anything else patches there. Unknown ids are ignored. */
export function toggleRoute(w: World, id: string, label: string): World {
  const cur = w.chans[id]?.src;
  if (cur === undefined) return w;
  return cur === label ? unpatchChannel(w, id) : patchChannelSrc(w, id, label);
}

/** Stage one matrix tap. Tapping the staged target again unstages it;
 * tapping the channel's current source stages an unpatch. Entries that
 * end up equal to the live value are dropped, so an empty map clears. */
export function stageRoute(w: World, id: string, label: string): World {
  const cur = w.chans[id]?.src;
  if (cur === undefined) return w;
  const staged: Record<string, string> = { ...(w.state.pendingPatch?.cells ?? {}) };
  if (staged[id] !== undefined) {
    if (staged[id] === label) delete staged[id];
    else staged[id] = label;
  } else if (cur === label) {
    staged[id] = '—';
  } else {
    staged[id] = label;
  }
  if (staged[id] === cur) delete staged[id];
  return withState(w, { pendingPatch: Object.keys(staged).length ? { cells: staged } : null });
}

/** Stage a 1:1 straight patch (inputs 1..32 to the group's 01..32).
 * Only real changes are staged; a no-op stages nothing. */
export function stagePreset(w: World, kind: 'oneToOne' | 'clear', group: RouteGroup): World {
  if (w.state.locked) return w;
  const staged: Record<string, string> = {};
  if (kind === 'oneToOne') {
    for (let n = 1; n <= 32; n++) {
      const id = 'in' + n;
      const label = group + ' ' + String(n).padStart(2, '0');
      if (w.chans[id] && w.chans[id].src !== label) staged[id] = label;
    }
  } else {
    for (let n = 1; n <= 48; n++) {
      const id = 'in' + n;
      if (w.chans[id] && w.chans[id].src !== '—') staged[id] = '—';
    }
  }
  return withState(w, { pendingPatch: Object.keys(staged).length ? { cells: staged } : null });
}

/** Drop everything staged without applying it. */
export function cancelPatch(w: World): World {
  if (!w.state.pendingPatch) return w;
  return withState(w, { pendingPatch: null });
}

/** Commit every staged change, then clear. Lock-checked. */
export function applyPatch(w: World): World {
  const cells = w.state.pendingPatch?.cells;
  if (!cells || w.state.locked) return w;
  let next = w;
  for (const [id, label] of Object.entries(cells)) {
    next = withChannel(next, id, c => { c.src = label; });
  }
  return withState(next, { pendingPatch: null });
}
/** Commit helper: straight 1:1 patch of inputs 1..32 to the group's
 * 01..32 sockets (the common stagebox reset). No bank jumping. */
export function patchOneToOne(w: World, group: RouteGroup): World {
  if (w.state.locked) return w;
  let next = w;
  for (let n = 1; n <= 32; n++) {
    const id = 'in' + n;
    const label = group + ' ' + String(n).padStart(2, '0');
    next = withChannel(next, id, c => { c.src = label; });
  }
  return withState(next, { patchGroup: group });
}

/** Matrix preset: unpatch all 48 inputs. Lock-checked. */
export function clearPatches(w: World): World {
  if (w.state.locked) return w;
  let next = w;
  for (let n = 1; n <= 48; n++) {
    next = withChannel(next, 'in' + n, c => { c.src = '—'; });
  }
  return next;
}

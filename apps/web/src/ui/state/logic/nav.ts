// uiLogic/nav: page navigation, bank/channel selection.
// Pure, rune-free logic split out of state/uiLogic.ts (one version of
// the logic lives here; uiLogic.ts re-exports it). DOM-free, node-testable.

import { bankByKey, bankDefs, clamp } from '../../fixtures/designData.ts';
import { isChannel, PAGE_MODES, type Mode, type Tab, type UiState, type World, withState } from './types.ts';

// ---------- navigation (port of openPage + header actions) ----------

export function openPage(w: World, mode: Mode, extra?: Partial<UiState>): World {
  const prev = PAGE_MODES.includes(w.state.mode) ? w.state.prevMode : w.state.mode;
  return withState(w, { mode, prevMode: prev, ...(extra ?? {}) });
}

export function goFx(w: World): World { return openPage(w, 'fx'); }
export function goRouting(w: World): World { return openPage(w, 'routing'); }
export function goConnect(w: World): World { return openPage(w, 'connect'); }
export function goSettings(w: World): World { return openPage(w, 'settings'); }
export function goOffline(w: World): World { return withState(w, { connected: 'offline' }); }

/** Port of goBack: back target depends on prevMode. */
export function backTo(w: World): Mode {
  return isChannel(w.state) ? 'mixer' : (w.state.prevMode === 'channel' ? 'channel' : 'mixer');
}

export function goBack(w: World): World {
  return withState(w, { mode: backTo(w) });
}

export function openScenes(w: World): World {
  if (w.state.mode === 'scenes') return w;
  return openPage(w, 'scenes', { scenePv: w.state.scene });
}

/** Input tab "Change patch in Routing" (port of goPatch closure). */
export function goPatch(w: World, id: string): World {
  const n = parseInt(id.replace(/^\D+/, ''), 10);
  const bank = Number.isFinite(n) ? Math.floor((n - 1) / 16) : 0;
  return openPage(w, 'routing', { patchSel: id, routeBank: clamp(bank, 0, 2) });
}

export function setBank(w: World, bank: string): World {
  bankByKey(bank); // throws on unknown keys, like the design would break
  return withState(w, { bank });
}

// ---------- bus selector popup (item 7) ----------

/** Open/close the master bus selector popup. */
export function setBusPopup(w: World, open: boolean): World {
  return withState(w, { busPopup: open });
}

/** Bank key containing a strip id (fixture banks; first match wins). */
export function bankForStrip(id: string): string | null {
  for (const b of bankDefs()) {
    if (b.items.some(it => it.id === id)) return b.key;
  }
  return null;
}

/** Pick a bus popup row: bank jumps to the row's bank; buses become the
 * SOF master (tap Main LR to leave SOF); popup closes. */
export function selectBusMaster(w: World, id: string): World {
  const bank = bankForStrip(id);
  let next = withState(w, { busPopup: false });
  if (bank) next = withState(next, { bank });
  const m = /^mix(\d+)$/.exec(id);
  if (m) {
    const n = parseInt(m[1]!, 10);
    if (n >= 1) return withState(next, { sof: true, mix: n - 1 });
  } else if (id === 'mainlr') {
    return withState(next, { sof: false });
  }
  return withState(next, { sof: false });
}

export function selectChannel(w: World, id: string): World {
  if (!w.chans[id]) return w;
  return withState(w, { sel: id, nameEdit: null });
}

/** Toggle the name-bar editor for a channel (also selects it). */
export function openNameEditor(w: World, id: string): World {
  if (!w.chans[id] || w.state.locked) return w;
  return withState(w, { sel: id, nameEdit: w.state.nameEdit === id ? null : id });
}

/** Close the name-bar editor. */
export function closeNameEditor(w: World): World {
  if (!w.state.nameEdit) return w;
  return withState(w, { nameEdit: null });
}

/** Channel prev/next arrows cycle over every EQ-capable channel id. */
export function channelOrder(): string[] {
  const order: string[] = [];
  bankDefs().forEach(b => b.items.forEach(it => { if (it.eq) order.push(it.id); }));
  return order;
}

export function stepChannel(w: World, delta: number): World {
  const order = channelOrder();
  const idx = Math.max(0, order.indexOf(w.state.sel));
  const nid = order[(idx + delta + order.length) % order.length];
  return withState(w, { sel: nid });
}

export function openEq(w: World, id: string): World {
  if (!w.chans[id]?.eq) return w;
  return withState(w, { sel: id, mode: 'channel', tab: 'eq' });
}

export function openDyn(w: World, id: string): World {
  if (!w.chans[id]?.eq) return w;
  return withState(w, { sel: id, mode: 'channel', tab: 'comp' });
}

export function setTab(w: World, tab: Tab): World {
  return withState(w, { tab });
}

export function setSelBand(w: World, band: number): World {
  return withState(w, { selBand: band });
}

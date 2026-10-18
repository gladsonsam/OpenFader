// S78 Routing screen: pure view-model builder.
//
// Exact port of the `routingVals` branch of the design's renderVals
// (docs/design/src/design-logic.js.txt). Components stay presentational:
// the screen binds this to uiStore state + chans and wires ui.do actions.
// Run: node --experimental-strip-types apps/web/src/ui/screens/routing/routingVm.test.ts

import {
  routeBankIds,
  srcTileLabels,
  type RouteGroup,
  type UiState,
} from '../../state/uiLogic.ts';
import type { Channel } from '../../fixtures/designData.ts';

export interface ChipVm {
  label: string;
  pressed: boolean;
  bg: string;
  fg: string;
}

export interface RouteBankVm extends ChipVm {
  index: number;
}

export interface PatchGroupVm extends ChipVm {
  key: RouteGroup;
}

export interface RouteTileVm {
  id: string;
  name: string;
  src: string;
  srcColor: string;
  pressed: boolean;
  border: string;
  bg: string;
}

export interface SrcTileVm {
  label: string;
  used: string;
  pressed: boolean;
  bg: string;
  fg: string;
  border: string;
}

export interface RoutingVm {
  banks: RouteBankVm[];
  tiles: RouteTileVm[];
  selName: string;
  groups: PatchGroupVm[];
  srcTiles: SrcTileVm[];
  matrix: RouteMatrix;
}

export const ROUTE_BANK_LABELS = ['1–16', '17–32', '33–48'];

const PATCH_GROUPS: Array<{ key: RouteGroup; label: string }> = [
  { key: 'Lcl', label: 'Local' },
  { key: 'Net', label: 'Network' },
  { key: 'USB', label: 'USB' },
];

function chipOf(label: string, on: boolean): { label: string; pressed: boolean; bg: string; fg: string } {
  return {
    label,
    pressed: on,
    bg: on ? 'var(--u4-text-1)' : 'var(--u4-btn-bg)',
    fg: on ? 'var(--u4-seg-on-fg)' : 'var(--u4-text-4)',
  };
}

export function buildRoutingVm(
  state: UiState,
  chans: Record<string, Channel>,
  /** Live source overlay (desk truth wins online): fixture id -> label. */
  liveSrc?: Record<string, string>,
): RoutingVm {
  const used = usedSources(chans, liveSrc);
  const selC = chans[state.patchSel];
  const selName = selC?.name ?? '';
  const selSrc = liveSrc?.[state.patchSel] ?? selC?.src ?? '';
  const banks: RouteBankVm[] = ROUTE_BANK_LABELS.map((label, b) => ({
    ...chipOf(label, state.routeBank === b),
    index: b,
  }));
  const tiles: RouteTileVm[] = routeBankIds(state.routeBank).map(id => {
    const c = chans[id];
    const on = id === state.patchSel;
    const src = liveSrc?.[id] ?? c.src;
    return {
      id,
      name: c.name,
      src,
      srcColor: src === '—' ? '#ff8a80' : '#5fe3f0',
      pressed: on,
      border: on ? '#ffffff' : 'var(--u4-line-faint)',
      bg: on ? '#1b2330' : 'var(--u4-well-bg)'
    };
  });
  const groups: PatchGroupVm[] = PATCH_GROUPS.map(g => ({
    ...chipOf(g.label, state.patchGroup === g.key),
    key: g.key,
  }));
  const srcTiles: SrcTileVm[] = srcTileLabels(state.patchGroup).map(label => {
    const isCur = selSrc === label;
    const u = used[label];
    return {
      label,
      used: isCur ? 'patched' : u ? '→ ' + u : '',
      pressed: isCur,
      bg: isCur ? '#5cdb5c' : u ? '#141b26' : 'var(--u4-well-bg)',
      fg: isCur ? '#10240f' : u ? 'var(--u4-text-4)' : 'var(--u4-text-7)',
      border: isCur ? '#5cdb5c' : 'var(--u4-line-faint)',
    };
  });
  return { banks, tiles, selName, groups, srcTiles, matrix: buildRouteMatrix(state, chans, liveSrc) };
}

/** Source label -> using channel name (last wins), over all 48 inputs. */
export function usedSources(chans: Record<string, Channel>, liveSrc?: Record<string, string>): Record<string, string> {
  const used: Record<string, string> = {};
  for (let n = 1; n <= 48; n++) {
    const c = chans['in' + n];
    if (c) used[liveSrc?.['in' + n] ?? c.src] = c.name;
  }
  return used;
}

export interface RouteMatrixRow {
  id: string;
  name: string;
  src: string;
}

export interface RouteMatrix {
  rows: RouteMatrixRow[];
  cols: string[];
  /** Source label -> using channel name (last wins). */
  used: Record<string, string>;
  /** Staged changes (channel id -> source, '—' = unpatch). */
  pending: { cells: Record<string, string> } | null;
}

/** Dante-style patchbay grid: all 48 input channels in one scroll, one
 * column per source of the selected group. The module derives per-cell
 * state (current / conflict / free / staged preview) from row sources,
 * the used map and the pending map. */
export function buildRouteMatrix(state: UiState, chans: Record<string, Channel>, liveSrc?: Record<string, string>): RouteMatrix {
  const cols = srcTileLabels(state.patchGroup);
  const rows: RouteMatrixRow[] = [];
  for (let n = 1; n <= 48; n++) {
    const id = 'in' + n;
    const c = chans[id];
    rows.push({ id, name: c?.name ?? id, src: liveSrc?.[id] ?? c?.src ?? '—' });
  }
  return { rows, cols, used: usedSources(chans, liveSrc), pending: state.pendingPatch };
}

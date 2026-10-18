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
    bg: on ? '#e8edf5' : '#2b3038',
    fg: on ? '#0b0f14' : '#c9d1dc',
  };
}

export function buildRoutingVm(state: UiState, chans: Record<string, Channel>): RoutingVm {
  const used: Record<string, string> = {};
  for (let n = 1; n <= 48; n++) {
    const c = chans['in' + n];
    if (c) used[c.src] = c.name;
  }
  const selC = chans[state.patchSel];
  const selName = selC?.name ?? '';
  const selSrc = selC?.src ?? '';
  const banks: RouteBankVm[] = ROUTE_BANK_LABELS.map((label, b) => ({
    ...chipOf(label, state.routeBank === b),
    index: b,
  }));
  const tiles: RouteTileVm[] = routeBankIds(state.routeBank).map(id => {
    const c = chans[id];
    const on = id === state.patchSel;
    return {
      id,
      name: c.name,
      src: c.src,
      srcColor: c.src === '—' ? '#ff8a80' : '#5fe3f0',
      pressed: on,
      border: on ? '#ffffff' : '#222a36',
      bg: on ? '#1b2330' : '#0a0f17',
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
      bg: isCur ? '#5cdb5c' : u ? '#141b26' : '#0a0f17',
      fg: isCur ? '#10240f' : u ? '#c9d1dc' : '#8792a4',
      border: isCur ? '#5cdb5c' : '#222a36',
    };
  });
  return { banks, tiles, selName, groups, srcTiles };
}

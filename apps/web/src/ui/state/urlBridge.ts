// D2 State package: fixture URL contract (PLAN decision 2).
// Fixture mode: `?ui=4&fixture=1&freeze=1&screen=<id>(&ch=2&bank=ch1&locked=1&sof=1&fine=1)`
// Screen ids: mixer | ch-input | ch-eq | ch-gate | ch-comp | ch-sends |
//             fx | routing | connection | settings | scenes
// `ch-eq | ch-gate | ch-comp | ch-sends | ch-input` map to mode `channel`
// with the matching tab. `connection` maps to mode `connect`.

import { bankDefs, type Channel } from '../fixtures/designData.ts';
import type { Mode, Tab, UiState } from './uiLogic.ts';

export type ScreenId =
  | 'mixer' | 'ch-input' | 'ch-eq' | 'ch-gate' | 'ch-comp' | 'ch-sends'
  | 'fx' | 'routing' | 'connection' | 'settings' | 'scenes';

export const SCREEN_IDS: ScreenId[] = [
  'mixer', 'ch-input', 'ch-eq', 'ch-gate', 'ch-comp', 'ch-sends',
  'fx', 'routing', 'connection', 'settings', 'scenes',
];

export interface FixtureParams {
  fixture: boolean;
  freeze: boolean;
  screen: ScreenId | null;
  ch: string | null;
  bank: string | null;
  locked: boolean;
  sof: boolean;
  fine: boolean;
}

const SCREEN_MODE_TAB: Record<ScreenId, { mode: Mode; tab: Tab | null }> = {
  mixer: { mode: 'mixer', tab: null },
  'ch-input': { mode: 'channel', tab: 'input' },
  'ch-eq': { mode: 'channel', tab: 'eq' },
  'ch-gate': { mode: 'channel', tab: 'gate' },
  'ch-comp': { mode: 'channel', tab: 'comp' },
  'ch-sends': { mode: 'channel', tab: 'sends' },
  fx: { mode: 'fx', tab: null },
  routing: { mode: 'routing', tab: null },
  connection: { mode: 'connect', tab: null },
  settings: { mode: 'settings', tab: null },
  scenes: { mode: 'scenes', tab: null },
};

export function screenToModeTab(screen: ScreenId): { mode: Mode; tab: Tab | null } {
  return SCREEN_MODE_TAB[screen];
}

function isOne(q: URLSearchParams, key: string): boolean {
  const v = q.get(key);
  return v === '1' || v === 'true';
}

export function parseFixtureUrl(search: string): FixtureParams {
  const q = new URLSearchParams(search.startsWith('?') ? search : '?' + search);
  const screenRaw = q.get('screen');
  const screen = SCREEN_IDS.includes(screenRaw as ScreenId) ? (screenRaw as ScreenId) : null;
  return {
    fixture: isOne(q, 'fixture'),
    freeze: isOne(q, 'freeze'),
    screen,
    ch: q.get('ch'),
    bank: q.get('bank'),
    locked: isOne(q, 'locked'),
    sof: isOne(q, 'sof'),
    fine: isOne(q, 'fine'),
  };
}

/** Channel id from the `ch` param: digits map to `in<n>` when it exists. */
export function chParamToId(ch: string | null, chans: Record<string, Channel>, fallback: string): string {
  if (!ch) return fallback;
  const id = /^\d+$/.test(ch) ? 'in' + parseInt(ch, 10) : ch;
  return chans[id] ? id : fallback;
}

function bankParamToKey(bank: string | null, fallback: string): string {
  if (bank && bankDefs().some(b => b.key === bank)) return bank;
  return fallback;
}

/**
 * Apply fixture URL overrides onto a base state. When a channel screen is
 * requested without `ch`, keep the base sel (default `in1`); when `ch` names
 * an FX return etc., sel follows it.
 */
export function initialStateFromUrl(
  search: string,
  base: UiState,
  chans: Record<string, Channel>,
): { state: UiState; fixture: boolean; freeze: boolean } {
  const p = parseFixtureUrl(search);
  const state: UiState = { ...base };
  if (p.screen) {
    const { mode, tab } = screenToModeTab(p.screen);
    state.mode = mode;
    if (tab) state.tab = tab;
    if (mode !== 'mixer' && mode !== 'channel') state.prevMode = 'mixer';
  }
  if (p.ch) state.sel = chParamToId(p.ch, chans, state.sel);
  if (p.bank) state.bank = bankParamToKey(p.bank, state.bank);
  if (p.locked) state.locked = true;
  if (p.sof) state.sof = true;
  if (p.fine) state.fine = true;
  return { state, fixture: p.fixture, freeze: p.freeze };
}

/** Serialize live state back to the fixture query (round-trip). */
export function stateToSearch(state: UiState): string {
  const q = new URLSearchParams();
  q.set('ui', '4');
  q.set('fixture', '1');
  const screen = modeTabToScreen(state.mode, state.tab);
  if (screen) q.set('screen', screen);
  const m = state.sel.match(/^in(\d+)$/);
  if (m) q.set('ch', m[1]);
  else q.set('ch', state.sel);
  q.set('bank', state.bank);
  if (state.locked) q.set('locked', '1');
  if (state.sof) q.set('sof', '1');
  if (state.fine) q.set('fine', '1');
  return '?' + q.toString();
}

export function modeTabToScreen(mode: Mode, tab: Tab): ScreenId | null {
  if (mode === 'channel') {
    return ({ input: 'ch-input', eq: 'ch-eq', gate: 'ch-gate', comp: 'ch-comp', sends: 'ch-sends' } as const)[tab] ?? null;
  }
  if (mode === 'connect') return 'connection';
  if (mode === 'mixer' || mode === 'fx' || mode === 'routing' || mode === 'settings' || mode === 'scenes') return mode;
  return null;
}

/** Lets screens signal fixture readiness (design-diff waits on this). */
export function markReady(): void {
  if (typeof window !== 'undefined') (window as unknown as Record<string, unknown>).__uiReady = true;
}

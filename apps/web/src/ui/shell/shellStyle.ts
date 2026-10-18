// H1 shell package: pure (rune-free, DOM-free) style helpers for the header
// toolbar and bank rail. Every colour below is copied verbatim from
// docs/design/src/design-template.html (header section + bankBtns in
// renderVals); the Svelte components consume these so node tests can pin
// them without a browser.

/** Unselected bank-rail label colours (TINT map in the design's renderVals). */
export const BANK_TINT: Record<string, string> = {
  ch1: '#7be27b',
  ch17: '#7be27b',
  ch33: '#7be27b',
  fx: '#7be27b',
  mix: '#5fe3f0',
  grp: '#8fb4ff',
  mtx: '#ea8cf2',
  dca: '#ff8a80',
};

export interface BankBtnStyle {
  bg: string;
  fg: string;
  border: string;
  weight: number;
}

/**
 * Bank-rail button colours (port of the bankBtns mapper in renderVals):
 * selected = the bank's own colour + dark/light text; unselected =
 * #161c26 tile with the bank's tint colour and a #2a323f border.
 */
export function bankBtnStyle(
  bank: { key: string; color: string; text: string },
  active: boolean,
): BankBtnStyle {
  if (active) return { bg: bank.color, fg: bank.text, border: bank.color, weight: 600 };
  return { bg: '#161c26', fg: BANK_TINT[bank.key] ?? '#e8edf5', border: '#2a323f', weight: 500 };
}

export interface TileStyle {
  bg: string;
  border: string;
}

/**
 * Header toolbar / scene-button tile colours: active page = #232b37 on
 * #5a6577, inactive = #0d1219 on #2a323f (tb() in the design's renderVals).
 */
export function tileStyle(active: boolean): TileStyle {
  return active ? { bg: '#232b37', border: '#5a6577' } : { bg: '#0d1219', border: '#2a323f' };
}

/** aria-current value used by every header toolbar button. */
export function ariaCurrent(active: boolean): string {
  return active ? 'page' : 'false';
}

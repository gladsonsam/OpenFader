// Channel insert view-model: the desk's per-strip FX-insert bypass switch
// and slot select for the selected channel (`{root}/insert/on|sel`,
// rig-verified on X-Air: `/ch/NN`, `/bus/N`, `/lr`).
//
// Pure over an injected live source (reads only): null source or null
// engine strip hides the card (fixture mode renders nothing, so the
// fixture baselines are untouched). Values are echo-confirmed only — null
// ("not confirmed yet") until the first attach snapshot or param echo; the
// UI shows that honestly instead of faking a value.
//
// The select is the desk's raw `0..=8` int (live ch15 = 7, ch16 = 8 —
// NOT 1..4 FX slots, so no slot mapping is asserted here). The desk has
// NO insert-position leaf, so none is modelled.

/** Select bounds (firmware truth 2026-10-20: 0..8 accepted, 9+ ignored). */
export const INSERT_SEL_MIN = 0;
export const INSERT_SEL_MAX = 8;

export interface InsertVm {
  /** Card title (`Insert · <name>`). */
  title: string;
  /** Bypass-switch leaf state. */
  onSupported: boolean;
  on: boolean | null;
  onLabel: string;
  onAria: string;
  onBg: string;
  onFg: string;
  /** Slot-select leaf state (raw 0..8). */
  selSupported: boolean;
  sel: number | null;
  selText: string;
  selAria: string;
  selMinusAria: string;
  selPlusAria: string;
  /** Honest-UI line: counts, unknown states, or why nothing is shown. */
  note: string;
}

/** Live reads the builder needs (EngineWorld shaped, fixture-free). */
export interface LiveInsertSource {
  /** Engine strip index for the selected fixture channel (null = unmapped). */
  engineCh: number | null;
  /** Desk name for honest notes (`xair`, `sim`, …). */
  driver: string;
  /** False where the desk has no insert bypass leaf: the row collapses. */
  onSupported: boolean;
  /** Confirmed bypass leaf (null = unknown). */
  insertOn(): boolean | null;
  /** False where the desk has no insert select leaf: the row collapses. */
  selSupported: boolean;
  /** Confirmed select 0..8 (null = unknown). */
  insertSel(): number | null;
}

/** Build the insert card vm (null = hide the card: offline, unmapped, or
// neither leaf exists on the desk). */
export function insertVmFor(src: LiveInsertSource | null): InsertVm | null {
  if (!src || src.engineCh === null) return null;
  if (!src.onSupported && !src.selSupported) return null;
  const on = src.onSupported ? src.insertOn() : null;
  const sel = src.selSupported ? src.insertSel() : null;
  const notes: string[] = [];
  if (!src.onSupported) notes.push('no insert bypass leaf');
  else if (on === null) notes.push('insert bypass not confirmed yet');
  if (!src.selSupported) notes.push('no insert select leaf');
  else if (sel === null) notes.push('insert select not confirmed yet');
  else if (!Number.isInteger(sel) || sel < INSERT_SEL_MIN || sel > INSERT_SEL_MAX) {
    notes.push(`desk select ${sel} outside 0..8`);
  }
  return {
    title: 'Insert',
    onSupported: src.onSupported,
    on,
    onLabel: on === null ? 'Insert ?' : on ? 'Insert IN' : 'Insert OUT',
    onAria: `Channel insert${on === null ? ', not confirmed yet' : on ? ', inserted' : ', bypassed'}`,
    onBg: on === true ? '#e8edf5' : '#2b3038',
    onFg: on === true ? '#0b0f14' : '#c9d1dc',
    selSupported: src.selSupported,
    sel,
    selText: sel === null ? '?' : String(sel),
    selAria: `Insert select${sel === null ? ', not confirmed yet' : ', ' + sel + ' of 0 to 8'}`,
    selMinusAria: 'Insert select down',
    selPlusAria: 'Insert select up',
    note: notes.length ? `${src.driver}: ${notes.join('; ')}.` : '',
  };
}

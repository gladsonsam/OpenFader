// Channel desk-config view-model: the desk's LR assign switch and automix
// weight for the selected channel (both newly verified on X-Air, caps-gated).
//
// Pure over an injected live source (reads only): null source or null
// engine strip hides the card (fixture mode renders nothing, so the
// fixture baselines are untouched). Values are echo-confirmed only — null
// ("not confirmed yet") until the first attach snapshot or param echo; the
// UI shows that honestly instead of faking a value.

export interface DeskVm {
  /** Card title (`Desk config · <name>`). */
  title: string;
  /** LR assign leaf state. */
  lrSupported: boolean;
  lrAssigned: boolean | null;
  lrLabel: string;
  lrAria: string;
  lrBg: string;
  lrFg: string;
  /** Automix weight leaf state (0..1). */
  automixSupported: boolean;
  automixWeight: number | null;
  automixText: string;
  automixAria: string;
  /** Honest-UI line: counts, unknown states, or why nothing is shown. */
  note: string;
}

/** Live reads the builder needs (EngineWorld shaped, fixture-free). */
export interface LiveDeskSource {
  /** Engine strip index for the selected fixture channel (null = unmapped). */
  engineCh: number | null;
  /** Desk name for honest notes (`xair`, `sim`, …). */
  driver: string;
  /** False where the desk has no LR assign leaf: the row collapses. */
  lrSupported: boolean;
  /** Confirmed LR assign leaf (null = unknown). */
  lrAssign(): boolean | null;
  /** False where the desk has no automix weight leaf: the row collapses. */
  automixSupported: boolean;
  /** Confirmed automix weight 0..1 (null = unknown). */
  automixWeight(): number | null;
}

/** Build the desk-config card vm (null = hide the card: offline, unmapped,
// or neither leaf exists on the desk). */
export function deskVmFor(src: LiveDeskSource | null): DeskVm | null {
  if (!src || src.engineCh === null) return null;
  if (!src.lrSupported && !src.automixSupported) return null;
  const lr = src.lrSupported ? src.lrAssign() : null;
  const weight = src.automixSupported ? src.automixWeight() : null;
  const notes: string[] = [];
  if (!src.lrSupported) notes.push('no LR assign leaf');
  else if (lr === null) notes.push('LR assign not confirmed yet');
  if (!src.automixSupported) notes.push('no automix weight leaf');
  else if (weight === null) notes.push('automix weight not confirmed yet');
  return {
    title: 'Desk config',
    lrSupported: src.lrSupported,
    lrAssigned: lr,
    lrLabel: lr === null ? 'LR ?' : lr ? 'LR IN' : 'LR OUT',
    lrAria: `LR assign${lr === null ? ', not confirmed yet' : lr ? ', assigned' : ', unassigned'}`,
    lrBg: lr === true ? '#e8edf5' : '#2b3038',
    lrFg: lr === true ? '#0b0f14' : '#c9d1dc',
    automixSupported: src.automixSupported,
    automixWeight: weight,
    automixText: weight === null ? '?' : Math.round(weight * 100) + '%',
    automixAria: `Automix weight${weight === null ? ', not confirmed yet' : ', ' + Math.round(weight * 100) + ' percent'}`,
    note: notes.length ? `${src.driver}: ${notes.join('; ')}.` : '',
  };
}

// Every inline SVG path/viewBox used in docs/design/src/design-template.html
// plus the device icons from the design logic (connectVals ICON map).
// Paths use the template's exact stroke/fill attributes; sizes come from the
// width/height on the template's <svg> elements (18/16/22/20/24).

export interface IconDef {
  viewBox: string;
  /** inner SVG markup (paths/circles/rects), copied from the template */
  body: string;
  /** default stroke width used in the template */
  strokeWidth: number;
  /** linecap used in the template (most are round) */
  linecap?: 'round' | 'butt' | 'square' | 'inherit';
  linejoin?: 'round' | 'miter' | 'bevel' | 'miter-clip' | 'arcs';
  /** true for the filled routing-grid dots */
  hasFillElements?: boolean;
}

const def = (
  viewBox: string,
  body: string,
  strokeWidth: number,
  extra: Partial<IconDef> = {},
): IconDef => ({ viewBox, body, strokeWidth, ...extra });

export const icons = {
  /** Back chevron (header Back button, 18x18, stroke 2.2) */
  back: def(
    '0 0 24 24',
    '<path d="M15 6l-6 6 6 6"></path>',
    2.2,
    { linecap: 'round', linejoin: 'round' },
  ),
  /** Lock (locked banner, 16x16, stroke 2.2) */
  lock: def(
    '0 0 24 24',
    '<rect x="5" y="11" width="14" height="10" rx="1.5"></rect><path d="M8 11V8a4 4 0 0 1 8 0v3"></path>',
    2.2,
    { linecap: 'round', linejoin: 'round' },
  ),
  /** Routing grid (header Routing button, 22x22, stroke 1.8, filled dots) */
  routing: def(
    '0 0 24 24',
    '<path d="M3 7h18M3 12h18M3 17h18M7 3v18M12 3v18M17 3v18"></path><circle cx="7" cy="12" r="1.8" fill="currentColor"></circle><circle cx="12" cy="17" r="1.8" fill="currentColor"></circle><circle cx="17" cy="7" r="1.8" fill="currentColor"></circle>',
    1.8,
    { linecap: 'round', hasFillElements: true },
  ),
  /** Connection plug (header Connect button, 22x22, stroke 2) */
  plug: def(
    '0 0 24 24',
    '<path d="M9 2v5M15 2v5M6 7h12v4a6 6 0 0 1-12 0zM12 17v5"></path>',
    2,
    { linecap: 'round', linejoin: 'round' },
  ),
  /** Settings gear (header Settings button, 22x22, stroke 1.8) */
  gear: def(
    '0 0 24 24',
    '<circle cx="12" cy="12" r="3"></circle><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"></path>',
    1.8,
    { linecap: 'round', linejoin: 'round' },
  ),
  /** Chevron down (scene dropdown button, 16x16, stroke #a7b1c0 2.2) */
  chevronDown: def(
    '0 0 24 24',
    '<path d="M6 9l6 6 6-6"></path>',
    2.2,
    { linecap: 'round', linejoin: 'round' },
  ),
  /** Console device icon (Connection device cards, 24x24, stroke 1.8) */
  console: def(
    '0 0 24 24',
    '<path d="M3 5h18v14H3zM7 9v6M11 9v6M15 9v6M19 9v6"></path>',
    1.8,
    { linecap: 'round', linejoin: 'round' },
  ),
  /** I/O device icon (Connection stage-box card, 24x24, stroke 1.8) */
  io: def(
    '0 0 24 24',
    '<path d="M4 7h16v10H4zM7 12h.01M10 12h.01M13 12h.01M16 12h.01"></path>',
    1.8,
    { linecap: 'round', linejoin: 'round' },
  ),
  /** Previous channel (channel strip pager, 20x20, stroke 2.2) */
  chevronLeft: def(
    '0 0 24 24',
    '<path d="M15 6l-6 6 6 6"></path>',
    2.2,
    { linecap: 'round', linejoin: 'round' },
  ),
  /** Next channel (channel strip pager, 20x20, stroke 2.2) */
  chevronRight: def(
    '0 0 24 24',
    '<path d="M9 6l6 6-6 6"></path>',
    2.2,
    { linecap: 'round', linejoin: 'round' },
  ),
} satisfies Record<string, IconDef>;

export type IconName = keyof typeof icons;

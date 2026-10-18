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
  /** Chevron down (scene dropdown button, 16x16, stroke var(--u4-text-5) 2.2) */
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
  /** Meter bridge (lucide audio-lines artwork, 24x24, stroke 2) */
  meters: def(
    '0 0 24 24',
    '<path d="M2 10v3" /><path d="M6 6v11" /><path d="M10 3v18" /><path d="M14 8v8" /><path d="M18 5v13" /><path d="M22 10v3" />',
    2,
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

/** Per-channel strip icons: Lucide artwork (ISC, https://lucide.dev),
 * vendored so the gig app stays offline-first. Same 24x24 stroke language
 * as the UI set above; channels opt in (null = no icon, the default). */
export const chanIcons = {
  /** Vocal / handheld mic (lucide: mic) */
  mic: def(
    '0 0 24 24',
    '<path d="M12 19v3" /><path d="M19 10v2a7 7 0 0 1-14 0v-2" /><rect x="9" y="2" width="6" height="13" rx="3" />',
    2,
    { linecap: 'round', linejoin: 'round' },
  ),
  /** Electric guitar (lucide: guitar) */
  guitar: def(
    '0 0 24 24',
    '<path d="m11.9 12.1 4.514-4.514" /><path d="M20.1 2.3a1 1 0 0 0-1.4 0l-1.114 1.114A2 2 0 0 0 17 4.828v1.344a2 2 0 0 1-.586 1.414A2 2 0 0 1 17.828 7h1.344a2 2 0 0 0 1.414-.586L21.7 5.3a1 1 0 0 0 0-1.4z" /><path d="m6 16 2 2" /><path d="M8.23 9.85A3 3 0 0 1 11 8a5 5 0 0 1 5 5 3 3 0 0 1-1.85 2.77l-.92.38A2 2 0 0 0 12 18a4 4 0 0 1-4 4 6 6 0 0 1-6-6 4 4 0 0 1 4-4 2 2 0 0 0 1.85-1.23z" />',
    2,
    { linecap: 'round', linejoin: 'round' },
  ),
  /** Drums (lucide: drum) */
  drums: def(
    '0 0 24 24',
    '<path d="m2 2 8 8" /><path d="m22 2-8 8" /><ellipse cx="12" cy="9" rx="10" ry="5" /><path d="M7 13.4v7.9" /><path d="M12 14v8" /><path d="M17 13.4v7.9" /><path d="M2 9v8a10 5 0 0 0 20 0V9" />',
    2,
    { linecap: 'round', linejoin: 'round' },
  ),
  /** Keys / piano (lucide: piano) */
  keys: def(
    '0 0 24 24',
    '<path d="M10 13v4" /><path d="M14 13v4" /><path d="M18 13v4" /><path d="M2 13h20" /><path d="M22 11.5A3.5 3.5 0 0018.5 8a3.52 3.52 0 01-3.173-2A7 7 0 002 9v10a2 2 0 002 2h16a2 2 0 002-2z" /><path d="M6 13v4" />',
    2,
    { linecap: 'round', linejoin: 'round' },
  ),
  /** Headphones / DJ (lucide: headphones) */
  headphones: def(
    '0 0 24 24',
    '<path d="M3 14h3a2 2 0 0 1 2 2v3a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-7a9 9 0 0 1 18 0v7a2 2 0 0 1-2 2h-1a2 2 0 0 1-2-2v-3a2 2 0 0 1 2-2h3" />',
    2,
    { linecap: 'round', linejoin: 'round' },
  ),
  /** Vinyl / playback (lucide: disc-3) */
  vinyl: def(
    '0 0 24 24',
    '<circle cx="12" cy="12" r="10" /><path d="M6 12c0-1.7.7-3.2 1.8-4.2" /><circle cx="12" cy="12" r="2" /><path d="M18 12c0 1.7-.7 3.2-1.8 4.2" />',
    2,
    { linecap: 'round', linejoin: 'round' },
  ),
  /** FX / sparkle (lucide: sparkles) */
  fx: def(
    '0 0 24 24',
    '<path d="M11.017 2.814a1 1 0 0 1 1.966 0l1.051 5.558a2 2 0 0 0 1.594 1.594l5.558 1.051a1 1 0 0 1 0 1.966l-5.558 1.051a2 2 0 0 0-1.594 1.594l-1.051 5.558a1 1 0 0 1-1.966 0l-1.051-5.558a2 2 0 0 0-1.594-1.594l-5.558-1.051a1 1 0 0 1 0-1.966l5.558-1.051a2 2 0 0 0 1.594-1.594z" /><path d="M20 2v4" /><path d="M22 4h-4" /><circle cx="4" cy="20" r="2" />',
    2,
    { linecap: 'round', linejoin: 'round' },
  ),
  /** General music note (lucide: music) — bass, horns, anything else */
  music: def(
    '0 0 24 24',
    '<path d="M21 16V3a1 1 0 0 0-1.28-.96l-10 2.917a1 1 0 0 0-.72.96V19" /><circle cx="18" cy="16" r="3" /><circle cx="6" cy="19" r="3" />',
    2,
    { linecap: 'round', linejoin: 'round' },
  ),
} satisfies Record<string, IconDef>;

export type ChanIconName = keyof typeof chanIcons;
/** Picker order (grid order in the name editor). */
export const CHAN_ICON_NAMES: ChanIconName[] = [
  'mic', 'guitar', 'drums', 'keys', 'headphones', 'vinyl', 'fx', 'music',
];

// L0 themes: token override sets applied as CSS variables on the app root.
// `design-dark` = the exact design = EMPTY override (tokens.css is the truth).
// Optional scoped custom CSS is injected prefixed with the app root selector;
// at-rules that escape scope (@import) and non-data url() are rejected.
//
// The app exposes these through Settings > Display (see settingsVm): the
// registry below is the single source of truth — add a theme by adding an
// entry; the setting row, AppFrame wiring and the demo gallery pick it up.

export type ThemePresetId = 'design-dark' | 'graphite' | 'tungsten' | 'daylight' | 'night-red';

/** Token overrides: --u4-* variable name (with or without leading --) -> value. */
export type TokenOverrides = Record<string, string>;

export interface Theme {
  id: string;
  /** Display name (Settings row + demo gallery). */
  label: string;
  overrides: TokenOverrides;
  css?: string;
}

export const APP_ROOT_SELECTOR = '.u4-frame';

export const PRESETS: Record<ThemePresetId, Theme> = {
  'design-dark': { id: 'design-dark', label: 'Console Blue', overrides: {} },
  graphite: {
    id: 'graphite',
    label: 'Graphite',
    overrides: {
      '--u4-rail-bg': '#101214',
      '--u4-page-bg': '#0c0d0e',
      '--u4-root-fg': '#e9e9e9',
      '--u4-panel-bg': '#141516',
      '--u4-panel-border': '#262728',
      '--u4-well-bg': '#08090a',
      '--u4-well-bg-2': '#0d0e0f',
      '--u4-tile-bg': '#101112',
      '--u4-tile-border': '#2a2b2d',
      '--u4-tile-bg-on': '#2a2b2d',
      '--u4-tile-border-on': '#565758',
      '--u4-bank-off': '#141518',
      '--u4-line-panel': '#262728',
      '--u4-line-row': '#1e1f20',
      '--u4-line-faint': '#232425',
      '--u4-line-ctl': '#3d3e40',
      '--u4-line-ctl-2': '#434446',
      '--u4-rail-border': '#333436',
      '--u4-divider': '#2b2c2e',
      '--u4-seg-border': '#3b3c3e',
      '--u4-seg-opt-bg': '#1c1d1f',
      '--u4-seg-on-bg': '#e9e9e9',
      '--u4-seg-on-fg': '#101010',
      '--u4-seg-off-bg': '#2b2c2e',
      '--u4-seg-off-fg': '#c6c6c6',
      '--u4-btn-bg': '#2b2c2e',
      '--u4-btn-fg': '#e9e9e9',
      '--u4-btn-border': '#3d3e40',
      '--u4-chip-on-bg': '#eeeeee',
      '--u4-chip-on-fg': '#101010',
      '--u4-text-1': '#e9e9e9',
      '--u4-text-2': '#efefef',
      '--u4-text-3': '#e2e2e2',
      '--u4-text-4': '#c6c6c6',
      '--u4-text-5': '#a3a3a3',
      '--u4-text-6': '#8c8c8c',
      '--u4-text-7': '#848484',
      '--u4-text-8': '#7a7a7a',
      '--u4-text-9': '#767676',
      '--u4-text-10': '#696969',
      '--u4-text-dim': '#5a5a5a',
      '--u4-pan-fill': '#45464a',
      '--u4-pan-notch': '#434446',
      '--u4-graph-unity': '#3f4145',
      '--u4-knob-track': '#26272b',
      '--u4-knob-hubline': '#343537',
      '--u4-route-head': '#151617',
      '--u4-route-alt': '#0b0c0d',
      '--u4-route-trace': '#1c1d20',
      '--u4-route-div': '#38393d',
      '--u4-route-bar': '#101114',
      '--u4-route-preview': '#17181c',
      '--u4-io-tile': '#1a1b1d',
    },
  },
  tungsten: {
    id: 'tungsten',
    label: 'Tungsten',
    overrides: {
      '--u4-rail-bg': '#120d08',
      '--u4-page-bg': '#0d0906',
      '--u4-root-fg': '#f0e8dc',
      '--u4-panel-bg': '#171009',
      '--u4-panel-border': '#33261a',
      '--u4-well-bg': '#080604',
      '--u4-well-bg-2': '#0d0906',
      '--u4-tile-bg': '#120c07',
      '--u4-tile-border': '#3a2c1e',
      '--u4-tile-bg-on': '#2e2119',
      '--u4-tile-border-on': '#6b5636',
      '--u4-bank-off': '#14100a',
      '--u4-line-panel': '#33261a',
      '--u4-line-row': '#241a10',
      '--u4-line-faint': '#2a2013',
      '--u4-line-ctl': '#4a3a26',
      '--u4-line-ctl-2': '#52402c',
      '--u4-rail-border': '#3d3222',
      '--u4-divider': '#33291b',
      '--u4-seg-border': '#463722',
      '--u4-seg-opt-bg': '#1e140c',
      '--u4-seg-on-bg': '#ece2d2',
      '--u4-seg-on-fg': '#171006',
      '--u4-seg-off-bg': '#2e2119',
      '--u4-seg-off-fg': '#c9bda6',
      '--u4-btn-bg': '#2e2119',
      '--u4-btn-fg': '#f0e8dc',
      '--u4-btn-border': '#4a3a26',
      '--u4-chip-on-bg': '#ece2d2',
      '--u4-chip-on-fg': '#171006',
      '--u4-text-1': '#f0e8dc',
      '--u4-text-2': '#f2ecdf',
      '--u4-text-3': '#e6dcc8',
      '--u4-text-4': '#c9bda6',
      '--u4-text-5': '#a3967f',
      '--u4-text-6': '#8a7d68',
      '--u4-text-7': '#7e7260',
      '--u4-text-8': '#756a58',
      '--u4-text-9': '#6e6350',
      '--u4-text-10': '#655a48',
      '--u4-text-dim': '#57503f',
      '--u4-pan-fill': '#4a3a26',
      '--u4-pan-notch': '#52402c',
      '--u4-graph-unity': '#4a4136',
      '--u4-knob-track': '#2a2118',
      '--u4-knob-hubline': '#3a2f20',
      '--u4-route-head': '#150f08',
      '--u4-route-alt': '#0b0805',
      '--u4-route-trace': '#1e150c',
      '--u4-route-div': '#453722',
      '--u4-route-bar': '#100b06',
      '--u4-route-preview': '#1c1408',
      '--u4-io-tile': '#1c130b',
    },
  },
  daylight: {
    id: 'daylight',
    label: 'Daylight',
    overrides: {
      '--u4-page-bg': '#e9ebef',
      '--u4-root-fg': '#131a24',
      '--u4-header-bg': '#ffffff',
      '--u4-header-border': '#d4dae2',
      '--u4-rail-bg': '#e4e8ee',
      '--u4-panel-bg': '#ffffff',
      '--u4-panel-border': '#c9d1dc',
      '--u4-well-bg': '#dfe4ea',
      '--u4-well-bg-2': '#d5dbe3',
      '--u4-tile-bg': '#f4f6f9',
      '--u4-tile-border': '#c2cad4',
      '--u4-tile-bg-on': '#1a2230',
      '--u4-tile-border-on': '#1a2230',
      '--u4-bank-off': '#d4dae2',
      '--u4-line-panel': '#c9d1dc',
      '--u4-line-row': '#d4dae2',
      '--u4-line-faint': '#d4dae2',
      '--u4-line-ctl': '#aeb6c2',
      '--u4-line-ctl-2': '#aeb6c2',
      '--u4-rail-border': '#aeb6c2',
      '--u4-divider': '#c2cad4',
      '--u4-seg-border': '#aeb6c2',
      '--u4-seg-opt-bg': '#d4dae2',
      '--u4-seg-on-bg': '#1a2230',
      '--u4-seg-on-fg': '#f4f6f9',
      '--u4-seg-off-bg': '#d4dae2',
      '--u4-seg-off-fg': '#3c4656',
      '--u4-btn-bg': '#232b37',
      '--u4-btn-fg': '#f4f6f9',
      '--u4-btn-border': '#232b37',
      '--u4-chip-on-bg': '#1a2230',
      '--u4-chip-on-fg': '#f4f6f9',
      '--u4-text-1': '#131a24',
      '--u4-text-2': '#1a2230',
      '--u4-text-3': '#232c3b',
      '--u4-text-4': '#3c4656',
      '--u4-text-5': '#55606f',
      '--u4-text-6': '#5d6a7e',
      '--u4-text-7': '#6b7688',
      '--u4-text-8': '#707b8d',
      '--u4-text-9': '#767f90',
      '--u4-text-10': '#5f6b7d',
      '--u4-text-dim': '#66727f',
      '--u4-pan-marker': '#1a2230',
      '--u4-pan-fill': '#9fb0c8',
      '--u4-pan-notch': '#8a93a1',
      '--u4-graph-unity': '#8a93a1',
      '--u4-knob-track': '#c2cad4',
      '--u4-knob-hubline': '#aeb6c2',
      '--u4-route-head': '#eef1f5',
      '--u4-route-alt': '#e6eaf0',
      '--u4-route-trace': '#d3d9e5',
      '--u4-route-div': '#b9c2cd',
      '--u4-route-bar': '#dfe3ea',
      '--u4-route-preview': '#cfd9e6',
      '--u4-io-tile': '#dde2e9',
      '--u4-cyan': '#0e8a99',
      '--u4-cyan-hi': '#0b7d8c',
    },
  },
  'night-red': {
    id: 'night-red',
    label: 'Night Red',
    overrides: {
      '--u4-page-bg': '#0a0304',
      '--u4-root-fg': '#f5d9d9',
      '--u4-panel-bg': '#160a0b',
      '--u4-panel-border': '#4a1d1d',
      '--u4-rail-bg': '#160a0b',
      '--u4-bank-off': '#1f1010',
      '--u4-well-bg': '#100506',
      '--u4-well-bg-2': '#140708',
      '--u4-tile-bg': '#160a0b',
      '--u4-tile-border': '#4a1d1d',
      '--u4-text-1': '#f5d9d9',
      '--u4-text-4': '#d8a8a8',
      '--u4-text-6': '#a97f7f',
      '--u4-focus': '#ff7a73',
      '--u4-link': '#ff7a73',
    },
  },
};

function normVar(name: string): string {
  return name.startsWith('--') ? name : `--${name}`;
}

/**
 * Apply token overrides as CSS variables on `root` (default: document-scoped
 * app root element). Returns the list of variables set. Never throws.
 */
export function applyTheme(
  theme: Theme,
  root?: { style: { setProperty(k: string, v: string): void; removeProperty(k: string): void } },
): string[] {
  try {
    const el = root ?? document.querySelector(APP_ROOT_SELECTOR) as unknown as {
      style: { setProperty(k: string, v: string): void };
    } | null;
    if (!el) return [];
    const set: string[] = [];
    for (const [k, v] of Object.entries(theme.overrides ?? {})) {
      if (typeof v !== 'string') continue;
      el.style.setProperty(normVar(k), v);
      set.push(normVar(k));
    }
    return set;
  } catch {
    return [];
  }
}

/** Remove previously applied overrides from `root`. Never throws. */
export function clearTheme(
  vars: string[],
  root?: { style: { removeProperty(k: string): void } },
): void {
  try {
    const el = root ?? document.querySelector(APP_ROOT_SELECTOR) as unknown as {
      style: { removeProperty(k: string): void };
    } | null;
    if (!el) return;
    for (const v of vars) el.style.removeProperty(normVar(v));
  } catch {
    /* ignore */
  }
}

// ---------- scoped custom CSS sanitiser ----------

const REJECT_AT = /@(import|charset|namespace|page|font-face)\b/i;
const URL_RE = /url\(\s*(['"]?)([^'")]+)\1\s*\)/gi;

/**
 * Scope custom CSS to the app root: prefix every selector with APP_ROOT_SELECTOR.
 * Rejects (returns { ok:false }) at-rules that escape scope (@import etc.) and
 * url() references that are not data: URLs (no external fetches from profiles).
 * Never throws.
 */
export function sanitizeCustomCss(css: string): { ok: true; css: string } | { ok: false; error: string } {
  try {
    if (typeof css !== 'string') return { ok: false, error: 'css must be a string' };
    if (REJECT_AT.test(css)) return { ok: false, error: 'at-rule not allowed in profile CSS' };
    let badUrl = false;
    css.replace(URL_RE, (_m, _q: string, target: string) => {
      if (!/^\s*data:/i.test(target)) badUrl = true;
      return _m;
    });
    if (badUrl) return { ok: false, error: 'url() must be a data: URL' };
    // Prefix each selector in each rule block with the app root selector.
    const scoped = css.replace(/(^|})([^{}]+)\{/g, (_m, sep: string, sel: string) => {
      const trimmed = sel.trim();
      if (trimmed.startsWith('@') || trimmed === '') return `${sep}${sel}{`;
      const prefixed = trimmed
        .split(',')
        .map(s => s.trim())
        .filter(Boolean)
        .map(s => (s.startsWith(APP_ROOT_SELECTOR) ? s : `${APP_ROOT_SELECTOR} ${s}`))
        .join(', ');
      return `${sep}${prefixed}{`;
    });
    return { ok: true, css: scoped };
  } catch (err) {
    return { ok: false, error: err instanceof Error ? err.message : 'invalid css' };
  }
}

// ---------- contrast checker (WCAG 2.1, 4.5:1 for normal text) ----------

function lum(hex: string): number | null {
  const m = /^#([0-9a-f]{6})$/i.exec(hex.trim());
  if (!m) return null;
  const c = [0, 2, 4].map(i => {
    const v = parseInt(m[1].slice(i, i + 2), 16) / 255;
    return v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4);
  });
  return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
}

/** Contrast ratio of two #rrggbb colours, or null when unparseable. */
export function contrastRatio(a: string, b: string): number | null {
  const la = lum(a);
  const lb = lum(b);
  if (la === null || lb === null) return null;
  const [hi, lo] = la >= lb ? [la, lb] : [lb, la];
  return (hi + 0.05) / (lo + 0.05);
}

/** True when the pair meets 4.5:1 (returns false on unparseable input). */
export function meetsContrast(a: string, b: string, min = 4.5): boolean {
  const r = contrastRatio(a, b);
  return r !== null && r >= min;
}

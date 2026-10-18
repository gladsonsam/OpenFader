// L0 themes: token override sets applied as CSS variables on the app root.
// `design-dark` = the exact design = EMPTY override (tokens.css is the truth).
// Optional scoped custom CSS is injected prefixed with the app root selector;
// at-rules that escape scope (@import) and non-data url() are rejected.

export type ThemePresetId = 'design-dark' | 'daylight' | 'night-red';

/** Token overrides: --u4-* variable name (with or without leading --) -> value. */
export type TokenOverrides = Record<string, string>;

export interface Theme {
  id: string;
  overrides: TokenOverrides;
  css?: string;
}

export const APP_ROOT_SELECTOR = '.u4-frame';

export const PRESETS: Record<ThemePresetId, Theme> = {
  'design-dark': { id: 'design-dark', overrides: {} },
  daylight: {
    id: 'daylight',
    overrides: {
      '--u4-page-bg': '#eef1f5',
      '--u4-root-fg': '#131a24',
      '--u4-panel-bg': '#ffffff',
      '--u4-panel-border': '#cfd6e0',
      '--u4-well-bg': '#e6ebf2',
      '--u4-well-bg-2': '#dde4ec',
      '--u4-text-1': '#131a24',
      '--u4-text-4': '#3c4656',
      '--u4-text-6': '#5d6a7e',
    },
  },
  'night-red': {
    id: 'night-red',
    overrides: {
      '--u4-page-bg': '#0a0304',
      '--u4-root-fg': '#f5d9d9',
      '--u4-panel-bg': '#160a0b',
      '--u4-panel-border': '#4a1d1d',
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

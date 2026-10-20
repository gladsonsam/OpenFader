// L0 themes: token override sets applied as CSS variables on the app root.
// `design-dark` is the default dark appearance; `daylight` is its light companion.
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
  'design-dark': {
    id: 'design-dark',
    label: 'Dark',
    overrides: {
      '--u4-page-bg': '#000000',
      '--u4-header-bg': '#0c0c0c',
      '--u4-header-border': '#2c2c2c',
      '--u4-rail-bg': '#0c0c0c',
      '--u4-panel-bg': '#191919',
      '--u4-panel-border': '#363636',
      '--u4-well-bg': '#0a0a0a',
      '--u4-well-bg-2': '#0a0a0a',
      '--u4-tile-bg': '#191919',
      '--u4-tile-border': '#363636',
      '--u4-tile-bg-on': '#333333',
      '--u4-tile-border-on': '#666666',
      '--u4-bank-off': '#0c0c0c',
      '--u4-line-panel': '#363636',
      '--u4-line-row': '#2a2a2a',
      '--u4-line-faint': '#363636',
      '--u4-divider': '#363636',
      '--u4-rail-border': '#454545',
      '--u4-red-btn-bg-off': '#292929',
      '--u4-red-btn-tx-off': '#e3e3e3',
      '--u4-red-line': '#494949',
      '--u4-red-btn': '#d33e4c',
      '--u4-red-hi': '#e05a66',
      '--u4-root-fg': '#f3f3f3',
      '--u4-text-1': '#f3f3f3',
      '--u4-text-2': '#f3f3f3',
      '--u4-text-3': '#e3e3e3',
      '--u4-line-ctl': '#454545',
      '--u4-line-ctl-2': '#454545',
      '--u4-btn-bg': '#2b2b2b',
      '--u4-btn-fg': '#f3f3f3',
      '--u4-btn-border': '#454545',
      '--u4-seg-opt-bg': '#222222',
      '--u4-seg-off-bg': '#2b2b2b',
      '--u4-seg-off-fg': '#cccccc',
      '--u4-seg-border': '#454545',
      '--u4-text-4': '#e3e3e3',
      '--u4-text-5': '#c4c4c4',
      '--u4-text-6': '#b5b5b5',
      '--u4-text-7': '#b5b5b5',
      '--u4-text-8': '#b5b5b5',
      '--u4-text-9': '#b5b5b5',
      '--u4-text-10': '#b5b5b5',
      '--u4-text-dim': '#aaaaaa',
      '--u4-pan-fill': '#666666',
      '--u4-pan-notch': '#949494',
      '--u4-fader-cap': '#898989',
      '--u4-strip-selected-ring': '#dedede',
      '--u4-focus': '#dddddd',
    },
  },
  "graphite": {
    id: 'graphite',
    label: 'Graphite',
    overrides: {
      '--u4-page-bg': '#17191c',
      '--u4-header-bg': '#202328',
      '--u4-rail-bg': '#202328',
      '--u4-bank-off': '#202328',
      '--u4-panel-bg': '#2d3137',
      '--u4-tile-bg': '#2d3137',
      '--u4-header-border': '#4b515a',
      '--u4-panel-border': '#4b515a',
      '--u4-tile-border': '#4b515a',
      '--u4-line-panel': '#4b515a',
      '--u4-line-row': '#4b515a',
      '--u4-line-faint': '#4b515a',
      '--u4-line-ctl': '#4b515a',
      '--u4-line-ctl-2': '#4b515a',
      '--u4-rail-border': '#4b515a',
      '--u4-divider': '#4b515a',
      '--u4-seg-border': '#4b515a',
      '--u4-btn-border': '#4b515a',
      '--u4-well-bg': '#1e2126',
      '--u4-well-bg-2': '#1e2126',
      '--u4-tile-bg-on': '#505a67',
      '--u4-tile-border-on': '#8995a5',
      '--u4-root-fg': '#f4f5f7',
      '--u4-text-1': '#f4f5f7',
      '--u4-text-2': '#f4f5f7',
      '--u4-text-3': '#f4f5f7',
      '--u4-text-4': '#d8dce3',
      '--u4-text-5': '#d8dce3',
      '--u4-seg-off-fg': '#d8dce3',
      '--u4-red-btn-tx-off': '#d8dce3',
      '--u4-text-6': '#b4bcc8',
      '--u4-text-7': '#b4bcc8',
      '--u4-text-8': '#b4bcc8',
      '--u4-text-9': '#b4bcc8',
      '--u4-text-10': '#b4bcc8',
      '--u4-text-dim': '#b4bcc8',
      '--u4-seg-opt-bg': '#393f47',
      '--u4-seg-off-bg': '#393f47',
      '--u4-btn-bg': '#393f47',
      '--u4-red-btn-bg-off': '#393f47',
      '--u4-btn-fg': '#f4f5f7',
      '--u4-seg-on-bg': '#505a67',
      '--u4-chip-on-bg': '#505a67',
      '--u4-seg-on-fg': '#f4f5f7',
      '--u4-chip-on-fg': '#f4f5f7',
      '--u4-red-line': '#4b515a',
      '--u4-red-btn': '#c83f4e',
      '--u4-red-hi': '#dc5260',
      '--u4-pan-marker': '#ffffff',
      '--u4-pan-fill': '#91a0b5',
      '--u4-pan-notch': '#b4bcc8',
      '--u4-graph-unity': '#4b515a',
      '--u4-knob-track': '#4b515a',
      '--u4-knob-hubline': '#4b515a',
      '--u4-route-head': '#202328',
      '--u4-route-bar': '#202328',
      '--u4-io-tile': '#202328',
      '--u4-route-alt': '#1e2126',
      '--u4-route-trace': '#1e2126',
      '--u4-route-div': '#4b515a',
      '--u4-route-preview': '#505a67',
      '--u4-fader-cap': '#a0adbd',
      '--u4-fader-well': '#262a30',
      '--u4-strip-selected-ring': '#c5d5eb',
      '--u4-focus': '#c5d5eb',
      '--u4-link': '#c5d5eb',
      '--u4-link-hover': '#f4f5f7',
      '--u4-eq-mini-curve': '#edcd71',
      '--u4-eq-mini-bypass': '#8791a0',
      '--u4-eq-mini-unity': '#4b515a',
      '--u4-master-bus-bg': '#393f47',
      '--u4-master-bus-fg': '#f4f5f7',
      '--u4-master-bus-border': '#4b515a',
      '--u4-channel-tab-bg': '#505a67',
      '--u4-channel-tab-fg': '#f4f5f7',
      '--u4-channel-tab-border': '#8995a5',
      '--u4-detail-curve': '#edcd71',
      '--u4-preamp-gain': '#ee6d78',
      '--u4-preamp-trim': '#7be27b',
      '--u4-choice-on-bg': '#505a67',
      '--u4-selected-surface': '#505a67',
      '--u4-selected-ink': '#f4f5f7',
      '--u4-graph-major': '#4b515a',
      '--u4-graph-minor': '#2d3137',
      '--u4-state-on': '#7be27b',
      '--u4-route-current-bg': '#505a67',
      '--u4-scene-name': '#f4f5f7',
      '--u4-scene-num': '#b4bcc8',
      '--u4-meter-cover': '#1e2126',
      '--u4-fader-rail': '#1e2126',
      '--u4-fader-rail-border': '#4b515a',
      '--u4-preamp-marker': '#d8dce3',
      '--u4-detail-panel': '#2d3137',
      '--u4-detail-well': '#1e2126',
      '--u4-detail-button': '#393f47',
      '--u4-detail-heading': '#f4f5f7',
      '--u4-detail-text': '#d8dce3',
      '--u4-detail-muted': '#b4bcc8',
      '--u4-detail-line': '#4b515a',
    },
  },
  "tungsten": {
    id: 'tungsten',
    label: 'Tungsten',
    overrides: {
      '--u4-page-bg': '#14110e',
      '--u4-header-bg': '#1e1a16',
      '--u4-rail-bg': '#1e1a16',
      '--u4-bank-off': '#1e1a16',
      '--u4-panel-bg': '#302a24',
      '--u4-tile-bg': '#302a24',
      '--u4-header-border': '#584c40',
      '--u4-panel-border': '#584c40',
      '--u4-tile-border': '#584c40',
      '--u4-line-panel': '#584c40',
      '--u4-line-row': '#584c40',
      '--u4-line-faint': '#584c40',
      '--u4-line-ctl': '#584c40',
      '--u4-line-ctl-2': '#584c40',
      '--u4-rail-border': '#584c40',
      '--u4-divider': '#584c40',
      '--u4-seg-border': '#584c40',
      '--u4-btn-border': '#584c40',
      '--u4-well-bg': '#1b1713',
      '--u4-well-bg-2': '#1b1713',
      '--u4-tile-bg-on': '#62503b',
      '--u4-tile-border-on': '#b19a77',
      '--u4-root-fg': '#f4ede2',
      '--u4-text-1': '#f4ede2',
      '--u4-text-2': '#f4ede2',
      '--u4-text-3': '#f4ede2',
      '--u4-text-4': '#ded1bd',
      '--u4-text-5': '#ded1bd',
      '--u4-seg-off-fg': '#ded1bd',
      '--u4-red-btn-tx-off': '#ded1bd',
      '--u4-text-6': '#c1af98',
      '--u4-text-7': '#c1af98',
      '--u4-text-8': '#c1af98',
      '--u4-text-9': '#c1af98',
      '--u4-text-10': '#c1af98',
      '--u4-text-dim': '#c1af98',
      '--u4-seg-opt-bg': '#42392f',
      '--u4-seg-off-bg': '#42392f',
      '--u4-btn-bg': '#42392f',
      '--u4-red-btn-bg-off': '#42392f',
      '--u4-btn-fg': '#f4ede2',
      '--u4-seg-on-bg': '#62503b',
      '--u4-chip-on-bg': '#62503b',
      '--u4-seg-on-fg': '#f4ede2',
      '--u4-chip-on-fg': '#f4ede2',
      '--u4-red-line': '#584c40',
      '--u4-red-btn': '#c83f4e',
      '--u4-red-hi': '#dc5260',
      '--u4-pan-marker': '#f8ecd8',
      '--u4-pan-fill': '#b49a72',
      '--u4-pan-notch': '#c1af98',
      '--u4-graph-unity': '#584c40',
      '--u4-knob-track': '#584c40',
      '--u4-knob-hubline': '#584c40',
      '--u4-route-head': '#1e1a16',
      '--u4-route-bar': '#1e1a16',
      '--u4-io-tile': '#1e1a16',
      '--u4-route-alt': '#1b1713',
      '--u4-route-trace': '#1b1713',
      '--u4-route-div': '#584c40',
      '--u4-route-preview': '#62503b',
      '--u4-fader-cap': '#b6a58b',
      '--u4-fader-well': '#25201b',
      '--u4-strip-selected-ring': '#e4c48e',
      '--u4-focus': '#e4c48e',
      '--u4-link': '#e4c48e',
      '--u4-link-hover': '#f4ede2',
      '--u4-eq-mini-curve': '#ecc777',
      '--u4-eq-mini-bypass': '#a18e73',
      '--u4-eq-mini-unity': '#584c40',
      '--u4-master-bus-bg': '#42392f',
      '--u4-master-bus-fg': '#f4ede2',
      '--u4-master-bus-border': '#584c40',
      '--u4-channel-tab-bg': '#62503b',
      '--u4-channel-tab-fg': '#f4ede2',
      '--u4-channel-tab-border': '#b19a77',
      '--u4-detail-curve': '#ecc777',
      '--u4-preamp-gain': '#ee6d78',
      '--u4-preamp-trim': '#7be27b',
      '--u4-choice-on-bg': '#62503b',
      '--u4-selected-surface': '#62503b',
      '--u4-selected-ink': '#f4ede2',
      '--u4-graph-major': '#584c40',
      '--u4-graph-minor': '#302a24',
      '--u4-state-on': '#7be27b',
      '--u4-route-current-bg': '#62503b',
      '--u4-scene-name': '#f4ede2',
      '--u4-scene-num': '#c1af98',
      '--u4-meter-cover': '#1b1713',
      '--u4-fader-rail': '#1b1713',
      '--u4-fader-rail-border': '#584c40',
      '--u4-preamp-marker': '#ded1bd',
      '--u4-detail-panel': '#302a24',
      '--u4-detail-well': '#1b1713',
      '--u4-detail-button': '#42392f',
      '--u4-detail-heading': '#f4ede2',
      '--u4-detail-text': '#ded1bd',
      '--u4-detail-muted': '#c1af98',
      '--u4-detail-line': '#584c40',
    },
  },
  "daylight": {
    id: 'daylight',
    label: 'Light',
    overrides: {
      '--u4-page-bg': '#c8cdd3',
      '--u4-header-bg': '#f0f1f3',
      '--u4-rail-bg': '#e4e7eb',
      '--u4-bank-off': '#e4e7eb',
      '--u4-panel-bg': '#e6e9ed',
      '--u4-tile-bg': '#e6e9ed',
      '--u4-header-border': '#b8c2cd',
      '--u4-panel-border': '#9da6b1',
      '--u4-tile-border': '#9da6b1',
      '--u4-line-panel': '#b8c2cd',
      '--u4-line-row': '#b8c2cd',
      '--u4-line-faint': '#b8c2cd',
      '--u4-line-ctl': '#b8c2cd',
      '--u4-line-ctl-2': '#b8c2cd',
      '--u4-rail-border': '#b8c2cd',
      '--u4-divider': '#b8c2cd',
      '--u4-seg-border': '#b8c2cd',
      '--u4-btn-border': '#b8c2cd',
      '--u4-well-bg': '#d5dbe2',
      '--u4-well-bg-2': '#d5dbe2',
      '--u4-tile-bg-on': '#cbd7e6',
      '--u4-tile-border-on': '#7388a2',
      '--u4-root-fg': '#19232f',
      '--u4-text-1': '#19232f',
      '--u4-text-2': '#19232f',
      '--u4-text-3': '#19232f',
      '--u4-text-4': '#354456',
      '--u4-text-5': '#354456',
      '--u4-seg-off-fg': '#354456',
      '--u4-red-btn-tx-off': '#f5f6f8',
      '--u4-text-6': '#526278',
      '--u4-text-7': '#526278',
      '--u4-text-8': '#526278',
      '--u4-text-9': '#526278',
      '--u4-text-10': '#526278',
      '--u4-text-dim': '#526278',
      '--u4-seg-opt-bg': '#e5eaf0',
      '--u4-seg-off-bg': '#e5eaf0',
      '--u4-btn-bg': '#e5eaf0',
      '--u4-red-btn-bg-off': '#454b54',
      '--u4-btn-fg': '#19232f',
      '--u4-seg-on-bg': '#3b434f',
      '--u4-chip-on-bg': '#3b434f',
      '--u4-seg-on-fg': '#ffffff',
      '--u4-chip-on-fg': '#ffffff',
      '--u4-red-line': '#626b77',
      '--u4-red-btn': '#c93446',
      '--u4-red-hi': '#dc5260',
      '--u4-pan-marker': '#233c59',
      '--u4-pan-fill': '#798da6',
      '--u4-pan-notch': '#526278',
      '--u4-graph-unity': '#b8c2cd',
      '--u4-knob-track': '#b8c2cd',
      '--u4-knob-hubline': '#b8c2cd',
      '--u4-route-head': '#f3f5f7',
      '--u4-route-bar': '#f3f5f7',
      '--u4-io-tile': '#f3f5f7',
      '--u4-route-alt': '#eef1f4',
      '--u4-route-trace': '#eef1f4',
      '--u4-route-div': '#b8c2cd',
      '--u4-route-preview': '#cbd7e6',
      '--u4-fader-cap': '#a6afb9',
      '--u4-fader-well': '#25282d',
      '--u4-strip-selected-ring': '#315b87',
      '--u4-focus': '#315b87',
      '--u4-link': '#315b87',
      '--u4-link-hover': '#19232f',
      '--u4-eq-mini-curve': '#896000',
      '--u4-eq-mini-bypass': '#738195',
      '--u4-eq-mini-unity': '#b8c2cd',
      '--u4-master-bus-bg': '#454b54',
      '--u4-master-bus-fg': '#ffffff',
      '--u4-master-bus-border': '#626b77',
      '--u4-channel-tab-bg': '#cbd7e6',
      '--u4-channel-tab-fg': '#19232f',
      '--u4-channel-tab-border': '#7388a2',
      '--u4-detail-curve': '#896000',
      '--u4-preamp-gain': '#a93842',
      '--u4-preamp-trim': '#327549',
      '--u4-choice-on-bg': '#3b434f',
      '--u4-selected-surface': '#cbd7e6',
      '--u4-selected-ink': '#19232f',
      '--u4-graph-major': '#b8c2cd',
      '--u4-graph-minor': '#d5dce4',
      '--u4-state-on': '#277440',
      '--u4-route-current-bg': '#ddefdf',
      '--u4-scene-name': '#19232f',
      '--u4-scene-num': '#526278',
      '--u4-meter-cover': '#353b43',
      '--u4-fader-rail': '#0a0b0d',
      '--u4-fader-rail-border': '#5d6672',
      '--u4-preamp-marker': '#526278',
      '--u4-detail-panel': '#e6e9ed',
      '--u4-detail-well': '#d5dbe2',
      '--u4-detail-button': '#e5eaf0',
      '--u4-detail-heading': '#19232f',
      '--u4-detail-text': '#354456',
      '--u4-detail-muted': '#526278',
      '--u4-detail-line': '#b8c2cd',
      '--u4-green-hi': '#277440',
      '--u4-cyan': '#126f7d',
      '--u4-cyan-hi': '#126f7d',
      '--u4-fx': '#7652ac',
      '--u4-curve-gold': '#896000',
      '--u4-dyn-gold': '#896000',
      '--u4-eq-orange': '#a35416',
      '--u4-eq-yellow': '#856400',
      '--u4-eq-cyan': '#126f7d',
      '--u4-eq-grey': '#526278',
      '--u4-amber': '#855b0b',
      '--u4-amber-hi': '#855b0b',
      '--u4-clip-dark': '#47292c',
      '--u4-master-line': '#ffe099',
    },
  },
  "night-red": {
    id: 'night-red',
    label: 'Night red',
    overrides: {
      '--u4-page-bg': '#0d080a',
      '--u4-header-bg': '#170e12',
      '--u4-rail-bg': '#170e12',
      '--u4-bank-off': '#170e12',
      '--u4-panel-bg': '#27191e',
      '--u4-tile-bg': '#27191e',
      '--u4-header-border': '#55343e',
      '--u4-panel-border': '#55343e',
      '--u4-tile-border': '#55343e',
      '--u4-line-panel': '#55343e',
      '--u4-line-row': '#55343e',
      '--u4-line-faint': '#55343e',
      '--u4-line-ctl': '#55343e',
      '--u4-line-ctl-2': '#55343e',
      '--u4-rail-border': '#55343e',
      '--u4-divider': '#55343e',
      '--u4-seg-border': '#55343e',
      '--u4-btn-border': '#55343e',
      '--u4-well-bg': '#130b0e',
      '--u4-well-bg-2': '#130b0e',
      '--u4-tile-bg-on': '#55313c',
      '--u4-tile-border-on': '#b17587',
      '--u4-root-fg': '#ecd0d4',
      '--u4-text-1': '#ecd0d4',
      '--u4-text-2': '#ecd0d4',
      '--u4-text-3': '#ecd0d4',
      '--u4-text-4': '#d7adb6',
      '--u4-text-5': '#d7adb6',
      '--u4-seg-off-fg': '#d7adb6',
      '--u4-red-btn-tx-off': '#d7adb6',
      '--u4-text-6': '#bb8d99',
      '--u4-text-7': '#bb8d99',
      '--u4-text-8': '#bb8d99',
      '--u4-text-9': '#bb8d99',
      '--u4-text-10': '#bb8d99',
      '--u4-text-dim': '#bb8d99',
      '--u4-seg-opt-bg': '#38242b',
      '--u4-seg-off-bg': '#38242b',
      '--u4-btn-bg': '#38242b',
      '--u4-red-btn-bg-off': '#38242b',
      '--u4-btn-fg': '#ecd0d4',
      '--u4-seg-on-bg': '#55313c',
      '--u4-chip-on-bg': '#55313c',
      '--u4-seg-on-fg': '#ecd0d4',
      '--u4-chip-on-fg': '#ecd0d4',
      '--u4-red-line': '#55343e',
      '--u4-red-btn': '#c83f4e',
      '--u4-red-hi': '#dc5260',
      '--u4-pan-marker': '#ecc6ce',
      '--u4-pan-fill': '#a27382',
      '--u4-pan-notch': '#bb8d99',
      '--u4-graph-unity': '#55343e',
      '--u4-knob-track': '#55343e',
      '--u4-knob-hubline': '#55343e',
      '--u4-route-head': '#170e12',
      '--u4-route-bar': '#170e12',
      '--u4-io-tile': '#170e12',
      '--u4-route-alt': '#130b0e',
      '--u4-route-trace': '#130b0e',
      '--u4-route-div': '#55343e',
      '--u4-route-preview': '#55313c',
      '--u4-fader-cap': '#aa7f89',
      '--u4-fader-well': '#1e1217',
      '--u4-strip-selected-ring': '#e0a0ae',
      '--u4-focus': '#e0a0ae',
      '--u4-link': '#e0a0ae',
      '--u4-link-hover': '#ecd0d4',
      '--u4-eq-mini-curve': '#d5a48e',
      '--u4-eq-mini-bypass': '#926f7a',
      '--u4-eq-mini-unity': '#55343e',
      '--u4-master-bus-bg': '#38242b',
      '--u4-master-bus-fg': '#ecd0d4',
      '--u4-master-bus-border': '#55343e',
      '--u4-channel-tab-bg': '#55313c',
      '--u4-channel-tab-fg': '#ecd0d4',
      '--u4-channel-tab-border': '#b17587',
      '--u4-detail-curve': '#d5a48e',
      '--u4-preamp-gain': '#ee6d78',
      '--u4-preamp-trim': '#7be27b',
      '--u4-choice-on-bg': '#55313c',
      '--u4-selected-surface': '#55313c',
      '--u4-selected-ink': '#ecd0d4',
      '--u4-graph-major': '#55343e',
      '--u4-graph-minor': '#27191e',
      '--u4-state-on': '#7be27b',
      '--u4-route-current-bg': '#55313c',
      '--u4-scene-name': '#ecd0d4',
      '--u4-scene-num': '#bb8d99',
      '--u4-meter-cover': '#130b0e',
      '--u4-fader-rail': '#130b0e',
      '--u4-fader-rail-border': '#55343e',
      '--u4-preamp-marker': '#d7adb6',
      '--u4-detail-panel': '#27191e',
      '--u4-detail-well': '#130b0e',
      '--u4-detail-button': '#38242b',
      '--u4-detail-heading': '#ecd0d4',
      '--u4-detail-text': '#d7adb6',
      '--u4-detail-muted': '#bb8d99',
      '--u4-detail-line': '#55343e',
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

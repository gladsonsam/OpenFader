// L0 tests: theme presets, CSS scoping/sanitiser, contrast checker.
import { strict as assert } from 'node:assert';
import {
  applyTheme,
  clearTheme,
  contrastRatio,
  meetsContrast,
  PRESETS,
  sanitizeCustomCss,
} from './theme.ts';

// --- presets: design-dark is the exact design = empty override ---
{
  assert.deepEqual(PRESETS['design-dark'].overrides, {});
  assert.ok(Object.keys(PRESETS['daylight'].overrides).length > 0);
  assert.ok(Object.keys(PRESETS['night-red'].overrides).length > 0);
}

// --- apply/clear on a fake root (no DOM needed) ---
{
  const vars = new Map<string, string>();
  const root = {
    style: {
      setProperty: (k: string, v: string) => vars.set(k, v),
      removeProperty: (k: string) => vars.delete(k),
    },
  };
  const set = applyTheme({ id: 't', label: 't', overrides: { 'u4-page-bg': '#fff', '--u4-panel-bg': '#000' } }, root);
  assert.deepEqual(set.sort(), ['--u4-page-bg', '--u4-panel-bg']);
  assert.equal(vars.get('--u4-page-bg'), '#fff');
  clearTheme(set, root);
  assert.equal(vars.size, 0);
  // never throws without a root/document
  assert.deepEqual(applyTheme({ id: 't', label: 't', overrides: {} }), []);
}

// --- sanitiser: scope prefixing, at-rule + url rejection ---
{
  const good = sanitizeCustomCss('.my-btn { color: red; } .a, .b { margin: 0; }');
  assert.equal(good.ok, true);
  const css = (good as { css: string }).css;
  assert.ok(css.includes('.u4-frame .my-btn'), css);
  assert.ok(css.includes('.u4-frame .a, .u4-frame .b'), css);
  // data: URLs pass; remote urls fail
  assert.equal(sanitizeCustomCss('.x{background:url(data:image/png;base64,AAA)}').ok, true);
  assert.equal(sanitizeCustomCss('.x{background:url(https://evil/x.png)}').ok, false);
  assert.equal(sanitizeCustomCss('.x{background:url(/local.png)}').ok, false);
  // escaping at-rules rejected
  for (const bad of ['@import "x";', '@charset "utf-8";', '@page { margin: 0; }', '@namespace x;']) {
    assert.equal(sanitizeCustomCss(bad).ok, false, bad);
  }
  // benign at-rules inside scope (@media) are kept but selectors still scoped
  const media = sanitizeCustomCss('@media (max-width: 600px) { .x { color: red; } }');
  assert.equal(media.ok, true);
  // garbage never throws
  assert.equal(sanitizeCustomCss(null as never).ok, false);
}

// --- contrast: black/white = 21, design pairs pass 4.5:1 ---
{
  assert.equal(contrastRatio('#000000', '#ffffff'), 21);
  assert.equal(contrastRatio('nope', '#fff'), null);
  // primary text on page bg, green ink on green button, on-dark on selected chip
  assert.equal(meetsContrast('#e8edf5', '#05070b'), true);
  assert.equal(meetsContrast('#10240f', '#5cdb5c'), true);
  assert.equal(meetsContrast('#0b0f14', '#e8edf5'), true);
  // secondary greys on dark bg also pass
  assert.equal(meetsContrast('#c9d1dc', '#05070b'), true);
  assert.equal(meetsContrast('#5d6878', '#05070b'), false); // dim text fails: honest signal
}

console.log('theme.test.ts: ok');

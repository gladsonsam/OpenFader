// H1 shell package: pure-logic tests (style helpers + outlet registry).
// Run: node --experimental-strip-types apps/web/src/ui/shell/shell.test.ts

import { strict as assert } from 'node:assert';
import { BANK_TINT, ariaCurrent, bankBtnStyle, tileStyle } from './shellStyle.ts';
import {
  clearShellScreens,
  getShellScreen,
  registerDesignScreens,
  registerShellScreen,
} from './registry.ts';

// bankBtnStyle mirrors the design's bankBtns mapper.
{
  const bank = { key: 'ch1', color: '#5cdb5c', text: '#10240f' };
  assert.deepEqual(bankBtnStyle(bank, true), {
    bg: '#5cdb5c',
    fg: '#10240f',
    border: '#5cdb5c',
    weight: 600,
  });
  assert.deepEqual(bankBtnStyle(bank, false), {
    bg: '#161c26',
    fg: '#7be27b',
    border: '#2a323f',
    weight: 500,
  });
  // Every bank key has an unselected tint colour.
  for (const key of ['ch1', 'ch17', 'ch33', 'fx', 'mix', 'grp', 'mtx', 'dca']) {
    assert.ok(BANK_TINT[key], `missing tint for ${key}`);
  }
  assert.equal(bankBtnStyle({ key: 'mix', color: '#22d3e6', text: '#062a30' }, false).fg, '#5fe3f0');
  assert.equal(bankBtnStyle({ key: 'dca', color: '#dc2626', text: '#ffffff' }, false).fg, '#ff8a80');
}

// tileStyle mirrors tb() in the design's renderVals.
assert.deepEqual(tileStyle(true), { bg: '#232b37', border: '#5a6577' });
assert.deepEqual(tileStyle(false), { bg: '#0d1219', border: '#2a323f' });
assert.equal(ariaCurrent(true), 'page');
assert.equal(ariaCurrent(false), 'false');

// Outlet registry: register / bulk register / lookup / clear.
{
  clearShellScreens();
  assert.equal(getShellScreen('mixer'), null);
  const fake = {} as never as Parameters<typeof registerShellScreen>[1];
  registerShellScreen('mixer', fake);
  assert.equal(getShellScreen('mixer'), fake);
  registerDesignScreens({ 'ch-eq': fake, fx: fake });
  assert.equal(getShellScreen('ch-eq'), fake);
  assert.equal(getShellScreen('fx'), fake);
  assert.equal(getShellScreen('routing'), null);
  clearShellScreens();
  assert.equal(getShellScreen('mixer'), null);
}

console.log('shell.test.ts: ok');

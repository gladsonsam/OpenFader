// Device-class breakpoints (companion to device.ts).
// Run: node --experimental-strip-types apps/web/src/ui/modules/device.test.ts
import { strict as assert } from 'node:assert';
import { deviceClassOf, PHONE_MAX_WIDTH, TABLET_MAX_WIDTH } from './device.ts';

assert.equal(PHONE_MAX_WIDTH, 599);
assert.equal(TABLET_MAX_WIDTH, 1099);

// Portrait phones (360-430 CSS px, incl. the 390 harness width).
for (const w of [320, 360, 390, 430, 599]) {
  assert.equal(deviceClassOf(w), 'phone', `width ${w}`);
}
// Portrait tablets (768-834), landscape phones, small landscape tablets.
for (const w of [600, 768, 834, 1024, 1099]) {
  assert.equal(deviceClassOf(w), 'tablet', `width ${w}`);
}
// The 1280x720 design canvas and wider.
for (const w of [1100, 1280, 1440, 1920]) {
  assert.equal(deviceClassOf(w), 'desktop', `width ${w}`);
}
// Unknown widths (SSR/node) fall back to the design layout.
for (const w of [0, -1, NaN, Infinity]) {
  assert.equal(deviceClassOf(w), 'desktop', `width ${String(w)}`);
}

console.log('device.test.ts: ok');

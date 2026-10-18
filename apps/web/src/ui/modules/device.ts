// Responsive device classes (MODULES.md §1 "Responsive").
//
// A layout may carry per-device-class `variants` (phone/tablet/desktop); the
// renderer picks the overlay by the `device` it is given. Screens feed this
// module's value into LayoutRenderer so narrow viewports get the phone/tablet
// structure. The 1280x720 desktop design always resolves to 'desktop', so the
// design-diff baselines are untouched.
//
// Breakpoints are on viewport width. The app is full-viewport (AppFrame fills
// the window and Shell divides it without sidebars), so viewport width == the
// layout container width in practice:
//   phone  <= 599  (portrait phones, 360-430 CSS px)
//   tablet <= 1099 (portrait tablets 768-834, landscape phones, small laptops)
//   desktop otherwise (the 1280 design canvas and wider).
//
// Run: node --experimental-strip-types apps/web/src/ui/modules/device.test.ts

import type { DeviceClass } from './types.ts';

/** Viewport widths at or below this are `phone`. */
export const PHONE_MAX_WIDTH = 599;
/** Viewport widths at or below this (and above phone) are `tablet`. */
export const TABLET_MAX_WIDTH = 1099;

/**
 * Device class for a viewport width. Non-positive/NaN widths (unknown, e.g.
 * SSR) resolve to 'desktop' so the default design layout always renders.
 */
export function deviceClassOf(widthPx: number): DeviceClass {
  if (!Number.isFinite(widthPx) || widthPx <= 0) return 'desktop';
  if (widthPx <= PHONE_MAX_WIDTH) return 'phone';
  if (widthPx <= TABLET_MAX_WIDTH) return 'tablet';
  return 'desktop';
}

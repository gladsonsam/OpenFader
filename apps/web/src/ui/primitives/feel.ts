// Feel helpers: haptics + reduced-motion, shared by touch interactions.
// Haptics use the vibration API (Android/Chromium tablets); unsupported
// platforms (iOS Safari) no-op. Keep ticks short and discrete — never on
// high-frequency drags (fader/pan moves), only on taps and commits.

/** Native haptic path (Tauri plugin on the tablet), registered once
 *  by the mobile bridge. Takes precedence over the vibration API. */
let nativeTap: (() => void) | null = null;

/** Register the native haptic path (mobile bridge only). */
export function registerNativeTap(fn: () => void): void {
  nativeTap = fn;
}

/** Fire a short haptic tick. Prefers native haptics, falls back to the
 *  vibration API (Android Chromium); no-op where neither exists. */
export function tap(ms = 10): void {
  if (nativeTap) {
    try {
      nativeTap();
    } catch {
      /* ignore */
    }
    return;
  }
  try {
    const nav = navigator as Navigator & { vibrate?: (p: number | number[]) => boolean };
    if (typeof nav.vibrate === 'function') nav.vibrate(ms);
  } catch {
    /* noop */
  }
}

/** True when the user prefers reduced motion (transitions should go flat). */
export const REDUCED_MOTION: boolean =
  typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches;

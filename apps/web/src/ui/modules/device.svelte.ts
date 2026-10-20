// Viewport device class as Svelte state (companion to device.ts).
//
// Screens call `useDeviceClass()` during component init and pass `device.now`
// into LayoutRenderer (which applies the layout's per-device `variants`) and
// into chrome components (Shell rail orientation, header density). Tracks
// window resize (covers rotation); with no window (SSR/node) it reports
// 'desktop' so non-browser renders always match the design baseline.
import { deviceClassOf, PHONE_MAX_WIDTH } from './device.ts';
import type { DeviceClass } from './types.ts';

export interface DeviceState {
  readonly now: DeviceClass;
  readonly processingPreviews: boolean;
}

export function useDeviceClass(): DeviceState {
  const handset = typeof navigator !== 'undefined' && /iPhone|iPod|Android.*Mobile/i.test(navigator.userAgent);
  let width = $state(typeof window === 'undefined' ? 1280 : window.innerWidth);
  let height = $state(typeof window === 'undefined' ? 720 : (window.visualViewport?.height ?? window.innerHeight));
  $effect(() => {
    const viewport = window.visualViewport;
    const onResize = (): void => {
      width = window.innerWidth;
      height = viewport?.height ?? window.innerHeight;
    };
    onResize();
    window.addEventListener('resize', onResize);
    viewport?.addEventListener('resize', onResize);
    return () => {
      window.removeEventListener('resize', onResize);
      viewport?.removeEventListener('resize', onResize);
    };
  });
  return {
    get now(): DeviceClass {
      return deviceClassOf(width);
    },
    get processingPreviews(): boolean {
      return !handset && width > PHONE_MAX_WIDTH && height > PHONE_MAX_WIDTH;
    },
  };
}

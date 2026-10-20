// Viewport device class as Svelte state (companion to device.ts).
//
// Screens call `useDeviceClass()` during component init and pass `device.now`
// into LayoutRenderer (which applies the layout's per-device `variants`) and
// into chrome components (Shell rail orientation, header density). Tracks
// window resize (covers rotation); with no window (SSR/node) it reports
// 'desktop' so non-browser renders always match the design baseline.
import { deviceClassOf } from './device.ts';
import type { DeviceClass } from './types.ts';

export interface DeviceState {
  readonly now: DeviceClass;
}

export function useDeviceClass(): DeviceState {
  let width = $state(typeof window === 'undefined' ? 1280 : window.innerWidth);
  $effect(() => {
    const onResize = (): void => {
      width = window.innerWidth;
    };
    onResize();
    window.addEventListener('resize', onResize);
    return () => window.removeEventListener('resize', onResize);
  });
  return {
    get now(): DeviceClass {
      return deviceClassOf(width);
    },
  };
}

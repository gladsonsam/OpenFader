// Import code, never mount hidden screens. One import unit per idle turn
// prevents the old all-packages burst from competing with live controls.
import { loadManifestModules, manifestModuleFiles } from '../modules/registry.ts';
import { screenNavigationPending, screenPreloadTasks, shellScreenReady } from './loadScreen.ts';
import type { ScreenId } from '../state/urlBridge.ts';

interface PreloadContext {
  screen: ScreenId;
  connected: boolean;
  connecting: boolean;
}

const mixingOrder: ScreenId[] = [
  'mixer', 'ch-input', 'ch-eq', 'ch-comp', 'ch-gate', 'ch-sends',
  'routing', 'fx', 'ch-groups', 'scenes', 'connection', 'settings',
];
const disconnectedOrder: ScreenId[] = [
  'connection', 'mixer', 'settings', 'ch-input', 'ch-eq', 'ch-comp',
  'ch-gate', 'ch-sends', 'routing', 'fx', 'ch-groups', 'scenes',
];

export function startScreenPreload(context: () => PreloadContext): () => void {
  if (typeof window === 'undefined' || typeof document === 'undefined') return () => {};
  const connection = (navigator as Navigator & {
    connection?: { saveData?: boolean; effectiveType?: string };
    deviceMemory?: number;
  });
  const constrained = connection.connection?.saveData ||
    /(^|-)2g$/.test(connection.connection?.effectiveType ?? '') ||
    (connection.deviceMemory !== undefined && connection.deviceMemory <= 2) ||
    (navigator.hardwareConcurrency > 0 && navigator.hardwareConcurrency <= 2);
  const gap = constrained ? 180 : 24;
  const idleWindow = constrained ? 500 : 120;
  // Construct only the next route's queue, inside its idle turn. Building
  // all twelve here synchronously parsed/validated profiles in onMount.
  const queues = new Map<ScreenId, Array<() => Promise<unknown>>>();
  // Preserve auto-discovered extensions and the full custom-layout palette,
  // but only after the normal navigation destinations have warmed.
  const registrations = import.meta.glob(['../**/register.ts', '../**/*-register.ts', '../**/*Register.ts']);
  const remaining = [
    ...manifestModuleFiles().map(file => () => loadManifestModules([file])),
    () => import('./meters/MeterBridge.svelte'),
    ...Object.values(registrations),
  ];
  let stopped = false;
  let busy = false;
  let painted = false;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let idle: number | undefined;
  let frame: number | undefined;
  const pointers = new Set<number>();
  let quietUntil = 0;

  function interacted(event: Event): void {
    if (event.type === 'pointerdown') pointers.add((event as PointerEvent).pointerId);
    if (event.type === 'pointerup' || event.type === 'pointercancel') pointers.delete((event as PointerEvent).pointerId);
    quietUntil = performance.now() + 150;
  }

  function blurred(): void { pointers.clear(); }

  function cancelScheduled(): void {
    clearTimeout(timer);
    timer = undefined;
    if (idle !== undefined) window.cancelIdleCallback?.(idle);
    idle = undefined;
    if (frame !== undefined) window.cancelAnimationFrame(frame);
    frame = undefined;
  }

  function schedule(delay = gap): void {
    if (stopped || busy || document.hidden) return;
    cancelScheduled();
    timer = setTimeout(() => {
      timer = undefined;
      // A timer watchdog also covers implementations that fail to deliver
      // idle callbacks while meters continuously animate.
      if (typeof window.requestIdleCallback === 'function') {
        idle = window.requestIdleCallback(run, { timeout: idleWindow });
        timer = setTimeout(run, idleWindow + 50);
      } else {
        run();
      }
    }, delay);
  }

  function run(deadline?: IdleDeadline): void {
    cancelScheduled();
    if (stopped || busy || document.hidden) return;
    const current = context();
    // Link/attach gets the main thread and network first. A route requested
    // mid-preload starts immediately; only the already-running import is
    // non-cancellable. No subsequent background import starts until it ends.
    const inputPending = (navigator as Navigator & {
      scheduling?: { isInputPending?: () => boolean };
    }).scheduling?.isInputPending?.();
    if (current.connecting || screenNavigationPending() || pointers.size ||
        performance.now() < quietUntil || inputPending ||
        (deadline && deadline.timeRemaining() < 4)) {
      schedule(100);
      return;
    }
    const order = [current.screen, ...(current.connected ? mixingOrder : disconnectedOrder)];
    let task: (() => Promise<unknown>) | undefined;
    for (const id of order) {
      if (shellScreenReady(id)) continue;
      let queue = queues.get(id);
      if (!queue) {
        queue = screenPreloadTasks(id);
        queues.set(id, queue);
      }
      task = queue.shift();
      if (task) break;
    }
    task ??= remaining.shift();
    if (!task) return;
    busy = true;
    Promise.resolve().then(task).catch(error => {
      // A speculative failure must not become a product error. The shared
      // loader evicts failures; actual navigation retains its retry state.
      console.warn('[ui4] preload failed:', error);
    }).finally(() => {
      busy = false;
      schedule();
    });
  }

  function afterPaint(): void {
    if (painted || stopped) return;
    painted = true;
    schedule(0);
  }

  function visible(): void {
    if (document.hidden) cancelScheduled();
    else if (painted) schedule();
    else waitForPaint();
  }

  function waitForPaint(): void {
    cancelScheduled();
    // Two frames allow the mounted route to paint before any speculative
    // imports. The fallback avoids starving on a stalled frame callback.
    timer = setTimeout(afterPaint, 250);
    if (typeof window.requestAnimationFrame === 'function') {
      frame = window.requestAnimationFrame(() => {
        frame = window.requestAnimationFrame(afterPaint);
      });
    }
  }

  document.addEventListener('visibilitychange', visible);
  const inputEvents = ['pointerdown', 'pointerup', 'pointercancel', 'keydown', 'wheel', 'touchmove'];
  for (const event of inputEvents) window.addEventListener(event, interacted, { capture: true, passive: true });
  window.addEventListener('blur', blurred);
  waitForPaint();
  return () => {
    stopped = true;
    cancelScheduled();
    document.removeEventListener('visibilitychange', visible);
    for (const event of inputEvents) window.removeEventListener(event, interacted, true);
    window.removeEventListener('blur', blurred);
  };
}

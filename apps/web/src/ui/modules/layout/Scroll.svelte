<!-- L0 scroll container: ONE element, flex in `direction` with overflow auto.
     W5c: `flex` + visual box props (min-width etc.) so the mixer strip row
     can own its scroll box in layout data (additive only; defaults unchanged).
     `dragScroll` (mixer bank): mouse-only drag-to-scroll. Touch/pen scroll
     natively via touch-action; mouse has no native drag-scroll, so the
     container owns clearly-horizontal mouse drags: it steals the gesture
     with pointer capture (an engaged child gets pointercancel and aborts
     cleanly), while clearly-vertical gestures are yielded to the child
     controls (faders/pans). Axis handoff mirrors the controls' own
     direction locks, so both sides agree on who owns the gesture. -->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { ModuleContext } from '../types.ts';
  import { boxStyle, gapCss, sizeCss, styleJoin, type BoxProps } from './css.ts';

  let {
    props = {},
    ctx: _ctx,
    nodeId,
    children,
  }: {
    props: {
      direction?: 'row' | 'col';
      gap?: number | string;
      width?: number | string;
      height?: number | string;
      flex?: number | string;
      /** Scroll-snap axis ('x' for the phone strip scroller). Absent = off. */
      snap?: 'x' | 'y';
      /** Mouse drag-to-scroll (mixer bank). Touch/pen always use native scroll. */
      dragScroll?: boolean;
    } & BoxProps;
    ctx: ModuleContext;
    nodeId: string;
    children?: Snippet;
  } = $props();

  const style = $derived(
    styleJoin([
      'display:flex',
      `flex-direction:${props.direction === 'row' ? 'row' : 'column'}`,
      'overflow:auto',
      // Shrink inside flex parents (never push the page taller) + keep
      // momentum scrolling on iOS. No-op when content fits (desktop).
      'min-width:0',
      'min-height:0',
      // The 48-strip pager parks a strip under the finger (mandatory);
      // every other snap scroller (channel tab rail) stays proximity so
      // it never fights the user mid-swipe.
      props.snap ? `scroll-snap-type:${props.snap} ${nodeId === 'strip-scroll' ? 'mandatory' : 'proximity'}` : undefined,
      gapCss(props.gap),
      sizeCss(props.width) ? `width:${sizeCss(props.width)}` : undefined,
      sizeCss(props.height) ? `height:${sizeCss(props.height)}` : undefined,
      props.flex !== undefined ? `flex:${props.flex}` : undefined,
      scrolling ? 'cursor:grabbing' : undefined,
      scrolling ? 'user-select:none' : undefined,
      ...boxStyle(props),
    ]),
  );

  let el: HTMLDivElement | null = $state(null);
  let gest: { id: number; x0: number; y0: number; sl: number } | null = null;
  let scrolling = $state(false);
  // Mouse-fling momentum: smoothed release velocity (px/frame at 60fps),
  // decayed per frame until it drops below the stop threshold. Touch/pen
  // keep native momentum; mouse has none, so the container emulates it.
  let flingV = 0;
  let flingRaf = 0;
  let lastX = 0;
  let lastT = 0;

  function cancelFling(): void {
    flingV = 0;
    if (flingRaf) {
      cancelAnimationFrame(flingRaf);
      flingRaf = 0;
    }
  }

  function down(e: PointerEvent): void {
    if (!e.isPrimary) return;
    // Any new press (mouse, touch, pen) or wheel kills a running fling:
    // the user owns the scroll position again.
    cancelFling();
    if (!props.dragScroll) return;
    if (e.pointerType !== 'mouse' || e.button !== 0) return;
    // Record only: never capture here, or child controls could not drag.
    gest = { id: e.pointerId, x0: e.clientX, y0: e.clientY, sl: el?.scrollLeft ?? 0 };
    lastX = e.clientX;
    lastT = e.timeStamp;
    flingV = 0;
  }
  function move(e: PointerEvent): void {
    if (!props.dragScroll || !gest || e.pointerId !== gest.id) return;
    if (e.buttons !== 1) {
      gest = null;
      scrolling = false;
      return;
    }
    if (scrolling) {
      if (el) el.scrollLeft = gest.sl - (e.clientX - gest.x0);
      // Smoothed per-frame velocity for the release fling.
      const now = e.timeStamp;
      const dt = Math.max(1, now - lastT);
      const inst = ((e.clientX - lastX) / dt) * 16;
      flingV = flingV * 0.7 + inst * 0.3;
      lastX = e.clientX;
      lastT = now;
      return;
    }
    // A child that already captured (fader locked vertical, pan locked
    // horizontal) owns this gesture: stand down. Walk the composed path:
    // the capture lives on the control root (button) while the target may
    // be an inner span (cap/grip), which never holds capture itself.
    const path = typeof e.composedPath === 'function' ? e.composedPath() : [];
    for (const n of path) {
      if (n === el) break;
      if (
        n &&
        typeof (n as Element).hasPointerCapture === 'function'
      ) {
        try {
          if ((n as Element).hasPointerCapture(e.pointerId)) {
            gest = null;
            return;
          }
        } catch {
          /* noop */
        }
      }
    }
    const ax = Math.abs(e.clientX - gest.x0);
    const ay = Math.abs(e.clientY - gest.y0);
    if (ay > 10 && ay > ax) {
      gest = null;
      return; // vertical: the child control owns it
    }
    if (ax <= 12 || ax <= ay) return; // slop: nobody owns it yet
    // Horizontal lock: steal the gesture. A pending child gets
    // pointercancel and aborts without emitting a value.
    try {
      el?.setPointerCapture(e.pointerId);
    } catch {
      /* noop */
    }
    scrolling = true;
    if (el) el.scrollLeft = gest.sl - (e.clientX - gest.x0);
  }
  function end(e: PointerEvent): void {
    if (e.pointerId !== gest?.id) return;
    // Explicit release: the horizontal lock stole this pointer via
    // setPointerCapture; hand it back so later gestures start clean.
    try {
      if (el?.hasPointerCapture(e.pointerId)) el.releasePointerCapture(e.pointerId);
    } catch {
      /* noop */
    }
    const wasScrolling = scrolling;
    const v = flingV;
    gest = null;
    scrolling = false;
    flingV = 0;
    // Release fling: keep gliding along the drag direction, decaying until
    // the snap — or the next press/wheel/touch — stops us.
    if (wasScrolling && Math.abs(v) > 1 && el) {
      const target = el;
      flingV = v;
      const step = (): void => {
        if (!target.isConnected) {
          flingRaf = 0;
          return;
        }
        target.scrollLeft -= flingV;
        flingV *= 0.95;
        if (Math.abs(flingV) < 0.5) {
          flingRaf = 0;
          return;
        }
        flingRaf = requestAnimationFrame(step);
      };
      flingRaf = requestAnimationFrame(step);
    }
  }
</script>

<div
  bind:this={el}
  data-node={nodeId}
  data-l0="scroll"
  style={style}
  onpointerdown={down}
  onpointermove={move}
  onpointerup={end}
  onpointercancel={end}
  onwheel={cancelFling}
  ontouchstart={cancelFling}
>
  {@render children?.()}
</div>

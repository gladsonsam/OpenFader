<!-- Pan bar, pixel-matched to the design's strip pan control.
     Strip: 30px, centre notch (top 9 / bottom 9), white 3px marker.
     Channel strip: 34px, notch top/bottom 10, plus the readout text
     variant at right (11px #8792a4, e.g. 'C', 'L28', 'R45').
     Null pan renders the design's empty placeholder well.
     touch-action pan-y (not none): vertical swipes scroll the page, the
     horizontal drag stays with the pan control. -->
<script lang="ts">
  import { panFromDrag } from '../../state/uiLogic.ts';
  import { panText } from '../../fixtures/designData.ts';
  import { panMarkerLeft } from './signalGeometry.ts';
  import { tap } from '../feel.ts';

  let {
    value,
    name,
    variant = 'strip',
    disabled = false,
    fine = false,
    oninput = null,
  }: {
    /** -1..1, 0 = centre; null = no pan (placeholder well). */
    value: number | null;
    /** Channel name for the aria label ('Pan Ch 2 R28' pattern). */
    name: string;
    /** 'strip' (mixer, h30, no text) or 'aside' (channel strip, h34 + readout). */
    variant?: 'strip' | 'aside';
    disabled?: boolean;
    /** Fine mode: drag moves a quarter as far (design's dragFine). */
    fine?: boolean;
    oninput?: ((v: number) => void) | null;
  } = $props();

  const h = variant === 'aside' ? 34 : 30;
  const notchTB = variant === 'aside' ? 10 : 9;

  const text = $derived(panText(value));
  const aria = $derived('Pan ' + name + (text ? ' ' + text : ''));
  const markerLeft = $derived(panMarkerLeft(value ?? 0));

  let el: HTMLElement | null = $state(null);
  // Tablet direction lock (mirror of Fader): touch/pen stays pending until
  // horizontal intent locks the pan drag. Vertical intent aborts so the page
  // can scroll natively (touch-action pan-y). Mouse drags immediately.
  let drag: {
    x0: number;
    y0: number;
    start: number;
    w: number;
    active: boolean;
    el: HTMLElement | null;
  } | null = null;
  // Touch fallback for double-tap reset (dblclick covers mouse).
  let lastTap = 0;

  function down(e: PointerEvent): void {
    if (disabled || value == null || !oninput) return;
    const w = el?.getBoundingClientRect().width ?? 1;
    const target = e.currentTarget as HTMLElement;
    // All pointers start pending with NO capture: no value change until
    // horizontal intent locks the pan drag (then we capture); vertical
    // intent aborts so the page can scroll natively.
    drag = { x0: e.clientX, y0: e.clientY, start: value, w, active: false, el: target };
  }
  function move(e: PointerEvent): void {
    if (!drag || disabled || !oninput) return;
    if (!drag.active) {
      const dx = e.clientX - drag.x0;
      const dy = e.clientY - drag.y0;
      const ax = Math.abs(dx);
      const ay = Math.abs(dy);
      // Clearly vertical -> let the page scroll, never touch the pan value.
      if (ay > 12 && ay > ax) {
        drag = null;
        return;
      }
      if (ax <= 10 || ax <= ay) return;
      try {
        drag.el?.setPointerCapture(e.pointerId);
      } catch {
        /* noop */
      }
      drag.active = true;
    }
    oninput(panFromDrag(drag.start, e.clientX - drag.x0, drag.w, fine));
  }
  function up(): void {
    drag = null;
  }
  function reset(): void {
    if (disabled || value == null || !oninput) return;
    tap();
    oninput(0);
  }
  function ontap(): void {
    const now = Date.now();
    if (now - lastTap < 350) reset();
    lastTap = now;
  }
  function key(e: KeyboardEvent): void {
    if (disabled || value == null || !oninput) return;
    const step = fine ? 0.005 : 0.02;
    let v: number | null = null;
    if (e.key === 'ArrowLeft' || e.key === 'ArrowDown') v = value - step;
    else if (e.key === 'ArrowRight' || e.key === 'ArrowUp') v = value + step;
    else if (e.key === 'PageDown') v = value - 0.2;
    else if (e.key === 'PageUp') v = value + 0.2;
    else if (e.key === 'Home') v = -1;
    else if (e.key === 'End') v = 1;
    else return;
    e.preventDefault();
    oninput(Math.round(Math.max(-1, Math.min(1, v)) * 100) / 100);
  }

</script>

{#if value == null}
  <div
    aria-hidden="true"
    data-ctl="pan-empty"
    style="height:{h}px;flex:none;border:1px solid #1a2230;border-radius:var(--u4-r-3)"
  ></div>
{:else}
  <button
    bind:this={el}
    data-ctl="pan"
    role="slider"
    aria-label={aria}
    aria-valuemin={-100}
    aria-valuemax={100}
    aria-valuenow={Math.round(value * 100)}
    aria-valuetext={text}
    aria-disabled={disabled ? 'true' : undefined}
    disabled={disabled || !oninput ? true : undefined}
    onpointerdown={down}
    onpointermove={move}
    onpointerup={up}
    onpointercancel={up}
    ondblclick={reset}
    onclick={ontap}
    onkeydown={key}
    style="height:{h}px;flex:none;position:relative;border:1px solid var(--u4-rail-border);border-radius:var(--u4-r-3);background:var(--u4-well-bg);padding:0;touch-action:pan-y;cursor:{disabled
      ? 'default'
      : 'ew-resize'};width:100%"
  >
    <span
      style="position:absolute;left:50%;top:{notchTB}px;bottom:{notchTB}px;width:1px;background:#3a4558"
    ></span>
    <!-- TODO token: pan centre-notch #3a4558 is not in tokens.css. -->
    <span
      style="position:absolute;top:4px;bottom:4px;width:3px;border-radius:1px;background:#ffffff;left:{markerLeft}%;transform:translateX(-50%)"
    ></span>
    {#if variant === 'aside'}
      <span
        style="position:absolute;right:5px;top:50%;transform:translateY(-50%);font-size:11px;color:var(--u4-text-7)"
        >{text}</span
      >
    {/if}
  </button>
{/if}

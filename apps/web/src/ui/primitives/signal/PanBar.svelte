<!-- Pan bar: two triangles meet at centre, with centre-out fill showing
     direction + amount and a white position marker.
     Channel strip: 34px, notch top/bottom 10, plus the readout text
     variant at right (11px var(--u4-text-7), e.g. 'C', 'L28', 'R45').
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

  const h = $derived(variant === 'aside' ? 34 : 30);
  const notchTB = $derived(variant === 'aside' ? 10 : 9);

  const text = $derived(panText(value));
  const aria = $derived('Pan ' + name + (text ? ' ' + text : ''));
  const markerLeft = $derived(panMarkerLeft(value ?? 0));
  // Center-out fill: from centre to the marker, so direction + amount read
  // instantly (nothing filled at centre).
  const fillLeft = $derived(50 + Math.min(0, value ?? 0) * 50);
  const fillW = $derived(Math.abs(value ?? 0) * 50);
  // Detent glow when effectively centred.
  const centred = $derived(value != null && Math.abs(value) < 0.04);

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
    style="height:{h}px;flex:none;border:1px solid var(--u4-rail-border);border-radius:var(--u4-r-3)"
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
    title={aria}
    style="height:{h}px;flex:none;position:relative;border:1px solid var(--u4-rail-border);border-radius:var(--u4-r-3);background:var(--u4-well-bg);padding:0;touch-action:pan-y;cursor:{disabled
      ? 'default'
      : 'ew-resize'};width:100%"
  >
    <svg
      viewBox="0 0 100 24"
      preserveAspectRatio="none"
      aria-hidden="true"
      style="position:absolute;inset:5px 6px;width:calc(100% - 12px);height:calc(100% - 10px);pointer-events:none"
    >
      <path d="M0 1L50 12L0 23ZM100 1L50 12L100 23Z" fill="var(--u4-pan-notch)" opacity="0.4" />
      {#if fillW > 0.5}
        <path
          d="M0 1L50 12L0 23ZM100 1L50 12L100 23Z"
          fill="var(--u4-pan-fill)"
          style={`clip-path:inset(0 ${100 - fillLeft - fillW}% 0 ${fillLeft}%)`}
        />
      {/if}
    </svg>
    <span
      aria-hidden="true"
      style="position:absolute;left:50%;top:{notchTB}px;bottom:{notchTB}px;width:{centred
        ? 2
        : 1}px;background:{centred ? 'var(--u4-text-1)' : 'var(--u4-pan-notch)'};box-shadow:{centred
        ? '0 0 4px rgba(232,237,245,.6)'
        : 'none'}"
    ></span>
    <!-- Pan centre notch uses the pan-notch token (themeable). -->
    <span
      aria-hidden="true"
      style="position:absolute;top:4px;bottom:4px;width:3px;border-radius:1px;background:var(--u4-pan-marker);left:{markerLeft}%;transform:translateX(-50%);box-shadow:0 0 4px rgba(255,255,255,.45)"
    ></span>
    {#if variant === 'aside'}
      <span
        style="position:absolute;right:5px;top:50%;transform:translateY(-50%);font-size:11px;color:var(--u4-text-7)"
        >{text}</span
      >
    {/if}
  </button>
{/if}

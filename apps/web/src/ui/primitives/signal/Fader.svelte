<!-- Vertical fader, pixel-matched to the design's strip fader.
     Exact metal cap (gradient + grip lines + centre line), scale labels,
     rail, meter column and dB readout. Local-first drag: pointer capture,
     dy mapped through uiLogic.faderFromDrag (0.25 factor in fine mode);
     Arrow/PageUp/PageDown/Home/End keys; double-tap resets to unity
5:      (0 dB) but ONLY when already near unity (gated window, so a stray
6:      double-tap far from 0 dB never jumps the fader).
     bank pager, vertical drags stay with the fader. Only presses that start
     on the metal cap grab the fader (rail/meter/track fall through to the
     scroller). A direction lock (~10px slop) applies to all pointers:
     horizontal intent aborts without touching the value; vertical intent
     drags. touch-action is split: the track is `auto` so touches starting
     off the cap scroll freely; the cap is `none` so a touch starting on
     it can never be stolen mid-gesture by the pager (pan-x let the pager
     win the scroll race on finger jitter and kill the drag via
     pointercancel after a few px). The cap tracks the finger 1:1 from a
     local live value during the drag, not just the echoed store value —
     painted synchronously to the cap transform (never via rAF/Svelte
     flush, which per-strip Meter loops can starve on phone GPUs) while
     commits throttle by wall clock with an exact flush on release.
     Variants: 'strip' (mixer, cap 36x50), 'aside' (channel strip,
     cap 46x60 + wider scale), 'master' (dual meter, lighter 40x52 cap,
     gold centre line). -->
<script lang="ts">
  import ScaleLabels from './ScaleLabels.svelte';
  import Meter, { type MeterSource } from './Meter.svelte';
  import { faderFromDrag } from '../../state/uiLogic.ts';
  import { dbToPos } from '../../fixtures/designData.ts';
  import { faderGeometry } from './signalGeometry.ts';
  import { tap } from '../feel.ts';

  let {
    value,
    dbText,
    ariaLabel,
    variant = 'strip',
    accent = 'chan',
    readColor = 'var(--u4-text-4)',
    disabled = false,
    fine = false,
    resetValue = dbToPos(0),
    meter = null,
    meterFrozen = false,
    oninput = null,
  }: {
    /** 0..1 fader position. */
    value: number;
    /** dB readout text, e.g. '-4.3' (fmtDb). */
    dbText: string;
    /** e.g. 'Fader Ch 1 -4.3 dB'. */
    ariaLabel: string;
    variant?: 'strip' | 'aside' | 'master';
    /** Cap centre-line colour: channel white, sends-on-fader cyan, master gold. */
    accent?: 'chan' | 'sof' | 'master';
    readColor?: string;
    disabled?: boolean;
    /** Fine mode: drag/keyboard move a quarter as far. */
    fine?: boolean;
    /** Unity reset target (default 0 dB); double-tap only fires near it. */
    resetValue?: number | null;
    /** Meter feed; null renders the silent (fully covered) meter. */
    meter?: MeterSource | null;
    meterFrozen?: boolean;
    oninput?: ((v: number) => void) | null;
  } = $props();

  const geo = faderGeometry(variant);
  const insets = geo.insets;
  const railLeft = geo.railLeft;
  const cap = geo.cap;
  // TODO tokens: fader cap metal gradients, grip-line pattern, rail black +
  // inset ring #1f2a3a, and the SOF glow rgba(34,211,230,.7) are not in
  // tokens.css; copied verbatim from the design template.
  const capBg =
    variant === 'master'
      ? 'linear-gradient(to bottom, #8a909a 0%, #565b64 7%, #3a3e46 44%, #22252b 50%, #3a3e46 56%, #565b64 93%, #737983 100%)'
      : 'linear-gradient(to bottom, #7a808a 0%, #4a4f58 7%, #30343b 44%, #1c1f24 50%, #30343b 56%, #4a4f58 93%, #646a73 100%)';
  const capShadow =
    variant === 'master'
      ? '0 8px 14px rgba(0,0,0,.75), 0 2px 3px rgba(0,0,0,.6), inset 0 1px 0 rgba(255,255,255,.3), inset 0 -1px 0 rgba(0,0,0,.7), inset 1px 0 0 rgba(255,255,255,.08), inset -1px 0 0 rgba(255,255,255,.08)'
      : '0 8px 14px rgba(0,0,0,.75), 0 2px 3px rgba(0,0,0,.6), inset 0 1px 0 rgba(255,255,255,.28), inset 0 -1px 0 rgba(0,0,0,.7), inset 1px 0 0 rgba(255,255,255,.08), inset -1px 0 0 rgba(255,255,255,.08)';
  // $derived: the accent can change after mount (e.g. sends-on-fader mode), M1 found these went stale as plain consts.
  const capLine = $derived(
    accent === 'master'
      ? 'var(--u4-master-line)'
      : accent === 'sof'
        ? 'var(--u4-cyan)'
        : '#ffffff',
  );
  const capGlow = $derived(
    accent === 'master'
      ? '0 0 6px var(--u4-master-line)'
      : accent === 'sof'
        ? '0 0 6px rgba(34,211,230,.7)'
        : '0 0 6px rgba(255,255,255,.35)',
  );
  const readH = geo.readH;
  const readFont = geo.readFont;

  const silent: MeterSource = {
    getLevel: () => -90,
    getPeak: () => null,
    getClip: () => false,
    showPeak: variant !== 'master',
  };

  let area: HTMLButtonElement | null = $state(null);
  let capEl: HTMLSpanElement | null = $state(null);
  let trackEl: HTMLSpanElement | null = $state(null);
  // Travel height of the cap track (px), measured for the
  // compositor-friendly cap position (fixed bottom:0 anchor + translateY).
  let trackH = $state(0);
  // Direction lock: touch/pen starts pending (no capture, no value
  // change) until the gesture declares itself. Vertical lock -> fader drag
  // (capture then). Horizontal lock -> abort without touching the value
  // (the cap is touch-action:none, so this just no-ops; the pager still
  // scrolls from any touch that starts off the cap). Mouse drags use the
  // same gate.
  let drag: {
    x0: number;
    y0: number;
    start: number;
    h: number;
    active: boolean;
    el: HTMLElement | null;
    pid: number;
  } | null = null;
  // Drag commit throttle: pointermove can fire faster than a phone webview
  // can commit (each oninput walks dispatchAction + store + layout
  // re-render). Leading edge commits immediately; the rest collapse to the
  // latest value, flushed at most every COMMIT_MS with an exact flush on
  // release. Time-based, never rAF-based: every strip runs its own Meter
  // canvas rAF loop, which can starve rAF on the phone GPU, and neither
  // the pixels nor the commits may wait for it.
  // liveV mirrors the latest value so Svelte's eventual flush converges;
  // the pixels themselves are painted synchronously in move() (paintCap)
  // so the cap tracks the finger at touch rate even when commits and
  // Svelte flushes crawl.
  let pendingV: number | null = null;
  let liveV: number | null = $state(null);
  let lastCommit = 0;
  let commitTimer = 0;
  const COMMIT_MS = 100;

  function nowMs(): number {
    return typeof performance !== 'undefined' ? performance.now() : Date.now();
  }

  /** Synchronous cap paint: bypasses Svelte flush + rAF starvation. */
  function paintCap(v: number): void {
    const el = capEl;
    const track = trackEl;
    if (!el || !track) return;
    const h = trackH || track.clientHeight || 0;
    if (!h) return;
    const y = Math.max(0, Math.min(1, v)) * h;
    el.style.transform = `translate(-50%, 50%) translateY(-${y.toFixed(1)}px)`;
  }

  function commitNow(v: number): void {
    pendingV = null;
    lastCommit = nowMs();
    oninput?.(v);
  }

  function scheduleTrailing(): void {
    if (commitTimer) return;
    commitTimer = setTimeout(() => {
      commitTimer = 0;
      if (pendingV != null && !disabled) commitNow(pendingV);
    }, COMMIT_MS);
  }

  function clearTrailing(): void {
    if (commitTimer) {
      clearTimeout(commitTimer);
      commitTimer = 0;
    }
  }
  // Compositor-friendly cap offset: fixed bottom:0 anchor, travel via
  // translateY(-capY px). capY == value-fraction * measured track height,
  // so the centre line lands exactly where bottom:{pct}% put it.
  // What the cap shows: the live finger value mid-drag, else the echo.
  const shown = $derived(Math.max(0, Math.min(1, liveV ?? value)));
  const capY = $derived(shown * trackH);
  // Gated unity reset: double-tap snaps to resetValue (0 dB) only when
  // the fader is already within this normalised window of it (~±3 dB
  // around unity on the design taper: 40 dB per unit near 0 dB).
  // no-op so a stray double-tap never jumps the fader.
  const RESET_RANGE = 0.08;
  // Whether the last press started on the cap. Track taps must never
  // reset the fader: double-tap reset (mouse dblclick / touch tap-tap)
  // is gated on this, since click/dblclick fire after pointerup cleared
  // `drag`.
  let downOnCap = false;
  let lastTap = 0;

  $effect(() => {
    const el = trackEl;
    if (!el) return;
    const measure = (): void => {
      trackH = el.clientHeight || 0;
    };
    measure();
    if (typeof ResizeObserver === 'undefined') return;
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  });

  function down(e: PointerEvent): void {
    if (disabled || !oninput) return;
    // A second finger must never steal an active drag: overwriting start
    // mid-gesture would jump the value and confuse the pointerId tracking.
    if (drag?.active) return;
    // Cap-only grab: a drag must START on the metal cap to move the fader.
    // Presses on the rail/meter/track fall through to the enclosing
    // scroller instead. Small pad keeps the 36px cap grabbable on touch.
    const cr = capEl?.getBoundingClientRect();
    if (cr) {
      const pad = 8;
      if (
        e.clientX < cr.left - pad ||
        e.clientX > cr.right + pad ||
        e.clientY < cr.top - pad ||
        e.clientY > cr.bottom + pad
      ) {
        downOnCap = false;
        return;
      }
    }
    downOnCap = true;
    const rect = area?.getBoundingClientRect();
    // Design's faderDown: h = max(80, rect.height - 60).
    const h = Math.max(80, (rect?.height ?? 140) - 60);
    const el = e.currentTarget as HTMLElement;
    // All pointers start pending with NO capture: no value change until
    // vertical intent locks the drag (then we capture); horizontal intent
    // aborts without touching the value.
    drag = { x0: e.clientX, y0: e.clientY, start: value, h, active: false, el, pid: e.pointerId };
  }
  function move(e: PointerEvent): void {
    if (!drag || disabled || !oninput) return;
    // Multi-touch: only the pointer that started the drag drives it. A
    // second finger's moves must never reset start or overwrite the value.
    if (e.pointerId !== drag.pid) return;
    if (!drag.active) {
      const dx = e.clientX - drag.x0;
      const dy = drag.y0 - e.clientY;
      const ax = Math.abs(dx);
      const ay = Math.abs(dy);
      // Clearly horizontal -> abort, never touch the value.
      if (ax > 12 && ax > ay) {
        drag = null;
        return;
      }
      // Still in slop, or not clearly vertical yet: no value change.
      if (ay <= 10 || ay <= ax) return;
      try {
        drag.el?.setPointerCapture(e.pointerId);
      } catch {
        /* noop */
      }
      drag.active = true;
    }
    const v = faderFromDrag(drag.start, drag.y0 - e.clientY, drag.h, fine);
    // Pixels first, synchronously: the cap tracks the finger at touch
    // rate regardless of Svelte flush / rAF / commit-backpressure state.
    liveV = v;
    paintCap(v);
    // Commits throttle by wall clock, never by rAF (starved on phone).
    if (nowMs() - lastCommit >= COMMIT_MS) {
      clearTrailing();
      commitNow(v);
    } else {
      pendingV = v;
      scheduleTrailing();
    }
  }
  function up(e?: PointerEvent): void {
    // A second finger lifting (or its capture loss) must not kill the drag.
    if (e && drag && e.pointerId !== drag.pid) return;
    // Flush the last throttled value so the final pixel is never lost.
    clearTrailing();
    let finalV: number | null = null;
    if (pendingV != null) {
      finalV = pendingV;
      pendingV = null;
      if (!disabled) {
        lastCommit = nowMs();
        oninput?.(finalV);
      }
    }
    // Hold the pixels at the final value: the echoed store value (just
    // flushed) is truth, but under load Svelte may flush before the new
    // value prop arrives — without this the cap snaps back on release.
    if (finalV != null) paintCap(finalV);
    // Drop the live value: the echoed store value (now flushed) is truth.
    // (Kept through the drag even if the echo lags; cleared here so a
    // rejected commit, e.g. a locked strip, can never stick the cap.)
    liveV = null;
    if (e && drag?.active && drag.el) {
      try {
        if (drag.el.hasPointerCapture?.(e.pointerId)) drag.el.releasePointerCapture(e.pointerId);
      } catch {
        /* noop */
      }
    }
    drag = null;
  }
  function reset(): void {
    if (disabled || !oninput || !downOnCap) return;
    const target = resetValue ?? dbToPos(0);
    // Gate: only snap when already near unity; far away, no-op.
    if (Math.abs(value - target) > RESET_RANGE) return;
    tap();
    oninput(target);
  }
  function ontap(): void {
    const now = Date.now();
    // Same gate as reset(): tap-tap only counts on the cap and near unity.
    if (now - lastTap < 350 && downOnCap) reset();
    lastTap = now;
  }
  function key(e: KeyboardEvent): void {
    if (disabled || !oninput) return;
    const k = fine ? 0.25 : 1;
    let v: number | null = null;
    if (e.key === 'ArrowUp' || e.key === 'ArrowRight') v = value + 0.01 * k;
    else if (e.key === 'ArrowDown' || e.key === 'ArrowLeft') v = value - 0.01 * k;
    else if (e.key === 'PageUp') v = value + 0.1 * k;
    else if (e.key === 'PageDown') v = value - 0.1 * k;
    else if (e.key === 'Home') v = 0;
    else if (e.key === 'End') v = 1;
    else return;
    e.preventDefault();
    oninput(Math.max(0, Math.min(1, v)));
  }
</script>

<div style="display:flex;flex-direction:column;width:100%;height:100%;opacity:{disabled ? 0.45 : 1}">
  <div
    style="height:{readH}px;flex:none;text-align:center;font-size:{readFont}px;color:{readColor}"
  >
    {dbText}
  </div>
  <button
    bind:this={area}
    role="slider"
    aria-label={ariaLabel}
    aria-valuemin={0}
    aria-valuemax={100}
    aria-valuenow={Math.round(shown * 100)}
    aria-valuetext={dbText + ' dB'}
    aria-disabled={disabled || !oninput ? 'true' : undefined}
    disabled={disabled || !oninput ? true : undefined}
    onpointerdown={down}
    onpointermove={move}
    onpointerup={up}
    onpointercancel={up}
    onlostpointercapture={up}
    ondblclick={reset}
    onclick={ontap}
    onkeydown={key}
    style="flex:1;position:relative;display:block;width:100%;min-height:0;padding:0;border:0;background:transparent;touch-action:auto;cursor:default}"
  >
    <span
      bind:this={trackEl}
      style="position:absolute;left:{insets.l}px;right:{insets.r}px;top:{insets.t}px;bottom:{insets.b}px;display:block"
    >
      <ScaleLabels variant={geo.scale} />
      <span
        style="position:absolute;left:{railLeft};width:4px;top:0;bottom:0;border-radius:2px;background:#000;box-shadow:inset 0 0 0 1px #1f2a3a"
      ></span>
      <Meter
        variant={geo.meter}
        source={meter ?? silent}
        frozen={meterFrozen}
      />
      <span
        bind:this={capEl}
        style="position:absolute;left:{cap.left};bottom:0;width:{cap.w}px;height:{cap.h}px;transform:translate(-50%, 50%) translateY(-{capY.toFixed(1)}px);will-change:transform;border-radius:3px;background:{capBg};box-shadow:{capShadow};touch-action:none;cursor:{disabled ||
        !oninput
          ? 'default'
          : 'ns-resize'}"
      >
        <span
          style="position:absolute;left:{cap.gripLR}px;right:{cap.gripLR}px;top:{cap.gripTB}px;height:{cap.gripH}px;border-radius:1px;background:repeating-linear-gradient(to bottom, rgba(255,255,255,.17) 0 1px, rgba(0,0,0,.4) 1px 3px)"
        ></span>
        <span
          style="position:absolute;left:{cap.gripLR}px;right:{cap.gripLR}px;bottom:{cap.gripTB}px;height:{cap.gripH}px;border-radius:1px;background:repeating-linear-gradient(to bottom, rgba(255,255,255,.17) 0 1px, rgba(0,0,0,.4) 1px 3px)"
        ></span>
        <span
          style="position:absolute;left:0;right:0;top:50%;height:3px;margin-top:-1.5px;background:{capLine};box-shadow:{capGlow}"
        ></span>
      </span>
    </span>
  </button>
</div>

<!-- Vertical fader, pixel-matched to the design's strip fader.
     Exact metal cap (gradient + grip lines + centre line), scale labels,
     rail, meter column and dB readout. Local-first drag: dy mapped through
     uiLogic.faderFromDrag (0.25 factor in fine mode); Arrow/PageUp/PageDown/
     Home/End keys; double-tap resets to unity (0 dB) but ONLY when already
     near unity (gated window, so a stray double-tap far from 0 dB never
     jumps the fader).

     Gesture arbitration with the strip pager (the fader column is tall, so
     swiping there is natural): vertically the fader wins, horizontally the
     pager does. Only presses that start on the metal cap grab the fader
     (rail/meter/track fall through to the scroller). A direction lock
     (~10px slop) applies to all pointers: horizontal intent aborts without
     touching the value, vertical intent drags. touch-action is split: the
     track is `auto` so touches starting off the cap scroll freely; the cap
     (and its 8px grab pad) is `none` so a touch starting on it can never be
     stolen mid-gesture by the pager.

     Gesture ownership: a press that starts on the cap installs
     window-level, capture-phase pointermove/pointerup/pointercancel
     listeners keyed to that pointerId, and they drive the whole gesture
     (removed on end/cancel and on unmount). Element-level moves plus the
     platform's implicit capture are NOT reliable: on WKWebView and on
     Android the element-level target can stop receiving moves — or emit a
     spurious lostpointercapture — while the finger is still down and
     window-level moves keep arriving. That is what made a fader stick after
     ~11-20px of travel (measured: an 80pt drag moved the value ~1dB
     instead of ~7dB). lostpointercapture never ends a drag; only
     pointerup/pointercancel of the tracked pointerId do. Touch pointers
     never call setPointerCapture (WKWebView drops the explicit capture
     within a frame); mouse/pen still take it explicitly so a drag that
     leaves the element keeps working, and release it on end.

     The cap tracks the finger 1:1 from a local live value during the drag,
     not just the echoed store value — painted synchronously to the cap
     transform (never via rAF/Svelte flush, which per-strip Meter loops can
     starve on phone GPUs) while commits throttle by wall clock with an
     exact flush on release.
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

  const geo = $derived(faderGeometry(variant));
  const insets = $derived(geo.insets);
  const railLeft = $derived(geo.railLeft);
  const cap = $derived(geo.cap);
  // TODO tokens: fader cap metal gradients, grip-line pattern, rail black +
  // inset ring #1f2a3a, and the SOF glow rgba(34,211,230,.7) are not in
  // tokens.css; copied verbatim from the design template.
  const capBg = $derived(
    variant === 'master'
      ? 'linear-gradient(to bottom, #8a909a 0%, #565b64 7%, #3a3e46 44%, #22252b 50%, #3a3e46 56%, #565b64 93%, #737983 100%)'
      : 'linear-gradient(to bottom, #7a808a 0%, #4a4f58 7%, #30343b 44%, #1c1f24 50%, #30343b 56%, #4a4f58 93%, #646a73 100%)',
  );
  const capShadow = $derived(
    variant === 'master'
      ? '0 8px 14px rgba(0,0,0,.75), 0 2px 3px rgba(0,0,0,.6), inset 0 1px 0 rgba(255,255,255,.3), inset 0 -1px 0 rgba(0,0,0,.7), inset 1px 0 0 rgba(255,255,255,.08), inset -1px 0 0 rgba(255,255,255,.08)'
      : '0 8px 14px rgba(0,0,0,.75), 0 2px 3px rgba(0,0,0,.6), inset 0 1px 0 rgba(255,255,255,.28), inset 0 -1px 0 rgba(0,0,0,.7), inset 1px 0 0 rgba(255,255,255,.08), inset -1px 0 0 rgba(255,255,255,.08)',
  );
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
  const readH = $derived(geo.readH);
  const readFont = $derived(geo.readFont);

  const silent: MeterSource = $derived({
    getLevel: () => -90,
    getPeak: () => null,
    getClip: () => false,
    showPeak: variant !== 'master',
  });

  let area: HTMLButtonElement | null = $state(null);
  let capEl: HTMLSpanElement | null = $state(null);
  let trackEl: HTMLSpanElement | null = $state(null);
  // Travel height of the cap track (px), measured for the
  // compositor-friendly cap position (fixed bottom:0 anchor + translateY).
  let trackH = $state(0);
  // Direction lock: every pointer starts pending (no value change) until
  // the gesture declares itself. Vertical lock -> fader drag. Horizontal
  // lock -> abort without touching the value (the cap is touch-action:none,
  // so this just no-ops; the pager still scrolls from any touch that starts
  // off the cap). Mouse drags use the same gate.
  //
  // The gesture is driven by window capture-phase listeners installed in
  // down() and keyed to `pid`, not by the button's own pointermove/up. This
  // survives webview target retargeting and bogus lostpointercapture.
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
  // Svelte flushes crawl. It is also the drag origin for the next grab
  // (down() uses `liveV ?? value`), so a quick re-grab before the store
  // echo lands cannot jump the fader backwards. On release it is held
  // until the echoed prop catches up (bounded by HOLD_MS for a rejected
  // commit), so a late echo can never snap the cap back under the finger.
  let pendingV: number | null = null;
  let liveV: number | null = $state(null);
  let lastCommit = 0;
  let commitTimer: ReturnType<typeof setTimeout> | 0 = 0;
  let holdTimer: ReturnType<typeof setTimeout> | 0 = 0;
  const COMMIT_MS = 100;
  // How long a released value may outlive its store echo. Long enough for a
  // loaded phone flush, short enough that a rejected commit (locked strip)
  // still returns the cap to the real value.
  const HOLD_MS = 600;

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

  function clearHold(): void {
    if (holdTimer) {
      clearTimeout(holdTimer);
      holdTimer = 0;
    }
  }

  /**
   * Keep the painted value across the release until the store echo lands:
   * under load Svelte can flush before the new `value` prop arrives, and
   * dropping the live value there makes the cap snap back. The fallback
   * timer clears it even if the commit was rejected (locked strip), so the
   * cap still returns to the real value eventually.
   */
  function holdLive(v: number): void {
    liveV = v;
    paintCap(v);
    clearHold();
    holdTimer = setTimeout(() => {
      holdTimer = 0;
      // If a new gesture is running it owns the pixels and will hold again
      // on its own release.
      if (!drag) liveV = null;
    }, HOLD_MS);
  }

  // Echo catch-up: once the value prop reflects the held live value, the
  // prop is truth again and the override can go.
  $effect(() => {
    const lv = liveV;
    if (lv == null || drag) return;
    if (Math.abs(value - lv) <= 0.0005) {
      clearHold();
      liveV = null;
    }
  });
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

  // Every move/up that belongs to the tracked gesture is seen on window in
  // the capture phase, so it does not matter which element the webview
  // retargets the pointer to (and lostpointercapture is never consulted).
  function windowMove(e: PointerEvent): void {
    if (!drag || e.pointerId !== drag.pid) return;
    move(e);
  }

  function windowUp(e: PointerEvent): void {
    if (!drag || e.pointerId !== drag.pid) return;
    up(e);
  }

  function listenWindow(): void {
    if (typeof window === 'undefined') return;
    window.addEventListener('pointermove', windowMove, true);
    window.addEventListener('pointerup', windowUp, true);
    window.addEventListener('pointercancel', windowUp, true);
  }

  function unlistenWindow(): void {
    if (typeof window === 'undefined') return;
    window.removeEventListener('pointermove', windowMove, true);
    window.removeEventListener('pointerup', windowUp, true);
    window.removeEventListener('pointercancel', windowUp, true);
  }

  // Unmount: no gesture listeners, timers or pending paint overrides may
  // outlive the component.
  $effect(() => {
    return () => {
      unlistenWindow();
      clearTrailing();
      clearHold();
    };
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
    // All pointers start pending with NO capture and no value change:
    // vertical intent locks the drag (mouse/pen then take an explicit
    // capture); horizontal intent aborts without touching the value. The
    // gesture itself is driven by the window capture listeners installed
    // below, keyed to this pointerId.
    drag = {
      x0: e.clientX,
      y0: e.clientY,
      // Freshest value: a quick re-grab before the Svelte echo lands must
      // start where the finger left the cap, not at the stale prop.
      start: liveV ?? value,
      h,
      active: false,
      el,
      pid: e.pointerId,
    };
    listenWindow();
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
        unlistenWindow();
        return;
      }
      // Still in slop, or not clearly vertical yet: no value change.
      if (ay <= 10 || ay <= ax) return;
      // Touch pointers are implicitly captured by the platform and
      // WKWebView drops an explicit setPointerCapture on a touch pointer
      // within a frame of granting it (measured in the simulator:
      // gotpointercapture, then lostpointercapture while the finger is
      // still down). Touch therefore never calls it; the window listeners
      // are what keep the gesture alive. Mouse/pen have no implicit
      // capture, so they still take the explicit grab so a drag that
      // leaves the element keeps working (and release it on end).
      try {
        if (e.pointerType !== 'touch') (capEl ?? drag.el)?.setPointerCapture(e.pointerId);
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
    // The window listener is already keyed to the tracked pointerId; this
    // guard also protects direct/manual calls. A second finger lifting must
    // never kill the drag.
    if (e && drag && e.pointerId !== drag.pid) return;
    unlistenWindow();
    clearTrailing();
    // Flush the last throttled value so the final pixel is never lost.
    let finalV: number | null = null;
    if (pendingV != null) {
      finalV = pendingV;
      pendingV = null;
      if (!disabled) {
        lastCommit = nowMs();
        oninput?.(finalV);
      }
    }
    const wasActive = drag?.active ?? false;
    // Hold the pixels at the final value until the echoed store value
    // arrives: under load Svelte may flush before the new prop lands, and
    // without the hold the cap snaps back on release. liveV also stays
    // available as the next drag's start in that window. A gesture that
    // never left the slop changes nothing, so any existing hold is left
    // alone (its timer/echo catch-up will clear it).
    if (wasActive) {
      if (finalV != null) holdLive(finalV);
      else if (liveV != null) holdLive(liveV);
    } else if (finalV != null) {
      holdLive(finalV);
    }
    if (e && drag) {
      for (const n of [capEl, drag.el]) {
        try {
          if (n?.hasPointerCapture?.(e.pointerId)) n.releasePointerCapture(e.pointerId);
        } catch {
          /* noop */
        }
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
    data-readout="fader"
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
          style="position:absolute;left:{cap.gripLR}px;right:{cap.gripLR}px;top:{cap.gripTB}px;height:{cap.gripH}px;border-radius:1px;background:repeating-linear-gradient(to bottom, rgba(255,255,255,.17) 0 1px, rgba(0,0,0,.4) 1px 3px);pointer-events:none"
        ></span>
        <span
          style="position:absolute;left:{cap.gripLR}px;right:{cap.gripLR}px;bottom:{cap.gripTB}px;height:{cap.gripH}px;border-radius:1px;background:repeating-linear-gradient(to bottom, rgba(255,255,255,.17) 0 1px, rgba(0,0,0,.4) 1px 3px);pointer-events:none"
        ></span>
        <span
          style="position:absolute;left:0;right:0;top:50%;height:3px;margin-top:-1.5px;background:{capLine};box-shadow:{capGlow};pointer-events:none"
        ></span>
        <!-- Grab pad: an 8px transparent hit extension so the small metal cap
             stays easy to grab. It MUST own `touch-action:none` itself:
             WKWebView resolves the pan gesture from the hit target's
             touch-action (not the cap's — touch-action is not inherited), so
             a hit on a decorative child left the pager free to claim the pan
             and cancel a vertical fader drag. -->
        <span
          aria-hidden="true"
          style="position:absolute;inset:-8px;display:block;touch-action:none"
        ></span>
      </span>
    </span>
  </button>
</div>

<!-- Vertical fader, pixel-matched to the design's strip fader.
     Exact metal cap (gradient + grip lines + centre line), scale labels,
     rail, meter column and dB readout. Local-first drag: pointer capture,
     dy mapped through uiLogic.faderFromDrag (0.25 factor in fine mode);
     Arrow/PageUp/PageDown/Home/End keys; double-tap resets to unity.
     touch-action pan-x (not none): horizontal swipes scroll an enclosing
     bank pager, vertical drags stay with the fader.
     Variants: 'strip' (mixer, cap 36x50), 'aside' (channel strip,
     cap 46x60 + wider scale), 'master' (dual meter, lighter 40x52 cap,
     gold centre line). -->
<script lang="ts">
  import ScaleLabels from './ScaleLabels.svelte';
  import Meter, { type MeterSource } from './Meter.svelte';
  import { faderFromDrag } from '../../state/uiLogic.ts';
  import { dbToPos } from '../../fixtures/designData.ts';
  import { faderGeometry } from './signalGeometry.ts';

  let {
    value,
    dbText,
    ariaLabel,
    variant = 'strip',
    accent = 'chan',
    readColor = '#c9d1dc',
    disabled = false,
    fine = false,
    resetValue = null,
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
    /** Double-tap reset target (default: unity, dbToPos(0) = 0.75). */
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
  const pct = $derived((Math.max(0, Math.min(1, value)) * 100).toFixed(2));

  const silent: MeterSource = {
    getLevel: () => -90,
    getPeak: () => null,
    getClip: () => false,
    showPeak: variant !== 'master',
  };

  let area: HTMLButtonElement | null = $state(null);
  let drag: { y: number; start: number; h: number } | null = null;
  let lastTap = 0;

  function down(e: PointerEvent): void {
    if (disabled || !oninput) return;
    try {
      (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    } catch {
      /* noop */
    }
    const rect = area?.getBoundingClientRect();
    // Design's faderDown: h = max(80, rect.height - 60).
    const h = Math.max(80, (rect?.height ?? 140) - 60);
    drag = { y: e.clientY, start: value, h };
  }
  function move(e: PointerEvent): void {
    if (!drag || disabled || !oninput) return;
    oninput(faderFromDrag(drag.start, drag.y - e.clientY, drag.h, fine));
  }
  function up(): void {
    drag = null;
  }
  function reset(): void {
    if (disabled || !oninput) return;
    oninput(resetValue ?? dbToPos(0));
  }
  function ontap(): void {
    const now = Date.now();
    if (now - lastTap < 350) reset();
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
    aria-valuenow={Math.round(value * 100)}
    aria-valuetext={dbText + ' dB'}
    aria-disabled={disabled || !oninput ? 'true' : undefined}
    disabled={disabled || !oninput ? true : undefined}
    onpointerdown={down}
    onpointermove={move}
    onpointerup={up}
    onpointercancel={up}
    ondblclick={reset}
    onclick={ontap}
    onkeydown={key}
    style="flex:1;position:relative;display:block;width:100%;min-height:0;padding:0;border:0;background:transparent;touch-action:pan-x;cursor:{disabled ||
    !oninput
      ? 'default'
      : 'ns-resize'}"
  >
    <span
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
        style="position:absolute;left:{cap.left};bottom:{pct}%;width:{cap.w}px;height:{cap.h}px;transform:translate(-50%, 50%);border-radius:3px;background:{capBg};box-shadow:{capShadow}"
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

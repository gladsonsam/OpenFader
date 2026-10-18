<!-- Ring knob, pixel-matched to the design's knobItem.
     Dark ring with a progress arc in the accent colour (design's
     knobDash/knobRot geometry: 103.67 arc over 138.23, -135°..+135°),
     dark hub, white pointer line, label above the value below the knob.
     Drag semantics per kind (all from uiLogic / designData, never
     re-derived): 'spec' input/dyn knobs via specFromDrag (180px
     full-scale, 0.25 in fine mode); 'eq' band knobs via eqParamFromDrag
     with the design's eqXOf/log norms; 'norm' FX-style 0..1 knobs.
     Arrow/PageUp/PageDown/Home/End keys; double-tap resets to default.
     Sizes: 58 (EQ cards), 68 (input), 80 (dyn), 92 (FX). -->
<script lang="ts">
  import {
    spec,
    toN,
    fromN,
    clamp,
    eqXOf,
    fmtF,
    fmtG,
    knobDash as dashOf,
    knobRot as rotOf,
  } from '../../fixtures/designData.ts';
  import { eqParamFromDrag, specFromDrag } from '../../state/uiLogic.ts';
  import { EQ_RESET_F, EQ_RESET_Q } from './signalGeometry.ts';

  let {
    specKey,
    value,
    label,
    color,
    size = 58,
    fx = false,
    defaultValue,
    disabled = false,
    fine = false,
    valueSize = null,
    mode = 'spec',
    eqBand = 1,
    eqParam = 'f',
    fmt = null,
    oninput = null,
  }: {
    /** fixtures/designData spec key for mode 'spec' ('gain', 'cthr', 'ratio', ...). */
    specKey: string;
    /** Raw value (spec/EQ units; normalised 0..1 in mode 'norm'). */
    value: number;
    label: string;
    /** Progress-arc accent colour. */
    color: string;
    size?: number;
    /** FX style: thinner ring (4px), larger hub, shorter pointer. */
    fx?: boolean;
    /** Double-tap reset target (spec/EQ units; normalised 0..1 in 'norm' mode). */
    defaultValue: number;
    disabled?: boolean;
    fine?: boolean;
    /** Value text size (EQ cards 12, channel 13, FX 15). */
    valueSize?: number | null;
    /** Drag/normalise semantics: 'spec' | 'eq' | 'norm'. */
    mode?: 'spec' | 'eq' | 'norm';
    /** EQ band index 0 (HPF)..4 for mode 'eq'. */
    eqBand?: number;
    /** EQ param for mode 'eq'. */
    eqParam?: 'f' | 'g' | 'q';
    /** Value formatter for mode 'norm' (default: percent). */
    fmt?: ((v: number) => string) | null;
    oninput?: ((v: number) => void) | null;
  } = $props();

  const s = spec(specKey);

  /** Normalised 0..1 position, per mode (design's knobItem norms). */
  function normOf(v: number): number {
    if (mode === 'norm') return clamp(v, 0, 1);
    if (mode === 'eq') {
      if (eqParam === 'f') return eqBand === 0 ? Math.log(v / 20) / Math.log(50) : eqXOf(v);
      if (eqParam === 'g') return (v + 15) / 30;
      return Math.log(v / 0.3) / Math.log(16 / 0.3);
    }
    return toN(s, v);
  }
  function valueOfNorm(nd: number): number {
    const nrm = clamp(nd, 0, 1);
    if (mode === 'norm') return Math.round(nrm * 1000) / 1000;
    if (mode === 'eq') {
      if (eqParam === 'f') {
        const f = eqBand === 0 ? 20 * Math.pow(50, nrm) : 20 * Math.pow(1000, nrm);
        return clamp(f, 20, eqBand === 0 ? 1000 : 20000);
      }
      if (eqParam === 'g') return Math.round(clamp(-15 + 30 * nrm, -15, 15) * 10) / 10;
      return Math.round(clamp(0.3 * Math.pow(16 / 0.3, nrm), 0.3, 16) * 100) / 100;
    }
    return fromN(s, nrm);
  }
  function fmtOf(v: number): string {
    if (mode === 'norm') return (fmt ?? ((x: number) => Math.round(x * 100) + '%'))(v);
    if (mode === 'eq') {
      if (eqParam === 'f') return fmtF(v);
      if (eqParam === 'g') return fmtG(v);
      return v.toFixed(2);
    }
    return s.fmt(v);
  }

  const n = $derived(clamp(normOf(value), 0, 1));
  const text = $derived(fmtOf(value));
  const aria = $derived(label + ' ' + text);
  const vSize = $derived(valueSize ?? (size >= 92 ? 15 : size <= 58 ? 12 : 13));
  const ringW = fx ? 4 : 5;
  const hubR = fx ? 16 : 15;
  const tipY = fx ? 14 : 15;
  const ptrW = fx ? 2.2 : 2.5;

  let drag: { y: number; startN: number; startV: number } | null = null;
  let lastTap = 0;

  function down(e: PointerEvent): void {
    if (disabled || !oninput) return;
    try {
      (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    } catch {
      /* noop */
    }
    drag = { y: e.clientY, startN: clamp(normOf(value), 0, 1), startV: value };
  }
  function move(e: PointerEvent): void {
    if (!drag || disabled || !oninput) return;
    const dy = drag.y - e.clientY;
    if (mode === 'eq') oninput(eqParamFromDrag(eqParam, eqBand, drag.startV, dy, fine));
    else if (mode === 'norm')
      oninput(valueOfNorm(drag.startN + (dy / 180) * (fine ? 0.25 : 1)));
    else oninput(specFromDrag(specKey, drag.startN, dy, fine));
  }
  function up(): void {
    drag = null;
  }
  function reset(): void {
    if (disabled || !oninput) return;
    if (mode === 'eq') {
      if (eqParam === 'f') oninput(eqBand === 0 ? 80 : EQ_RESET_F[eqBand]);
      else if (eqParam === 'g') oninput(0);
      else oninput(EQ_RESET_Q[eqBand]);
      return;
    }
    oninput(defaultValue);
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
    if (e.key === 'ArrowUp' || e.key === 'ArrowRight') v = valueOfNorm(normOf(value) + 0.02 * k);
    else if (e.key === 'ArrowDown' || e.key === 'ArrowLeft')
      v = valueOfNorm(normOf(value) - 0.02 * k);
    else if (e.key === 'PageUp') v = valueOfNorm(normOf(value) + 0.1 * k);
    else if (e.key === 'PageDown') v = valueOfNorm(normOf(value) - 0.1 * k);
    else if (e.key === 'Home' || e.key === 'End') {
      e.preventDefault();
      if (mode === 'eq') {
        if (eqParam === 'f') oninput(e.key === 'Home' ? 20 : eqBand === 0 ? 1000 : 20000);
        else if (eqParam === 'g') oninput(e.key === 'Home' ? -15 : 15);
        else oninput(e.key === 'Home' ? 0.3 : 16);
        return;
      }
      if (mode === 'norm') {
        oninput(e.key === 'Home' ? 0 : 1);
        return;
      }
      oninput(e.key === 'Home' ? s.min : s.max);
      return;
    } else return;
    e.preventDefault();
    if (v != null) oninput(v);
  }
</script>

<div
  style="display:flex;flex-direction:column;align-items:center;gap:1px;min-width:0;opacity:{disabled
    ? 0.45
    : 1}"
>
  <button
    role="slider"
    aria-label={aria}
    aria-valuetext={text}
    aria-disabled={disabled || !oninput ? 'true' : undefined}
    disabled={disabled || !oninput ? true : undefined}
    onpointerdown={down}
    onpointermove={move}
    onpointerup={up}
    onpointercancel={up}
    ondblclick={reset}
    onclick={ontap}
    onkeydown={key}
    style="width:{size}px;height:{size}px;padding:0;border:0;background:transparent;touch-action:none;cursor:{disabled ||
    !oninput
      ? 'default'
      : 'ns-resize'}"
  >
    <svg width={size} height={size} viewBox="0 0 56 56" aria-hidden="true">
      <!-- TODO token: knob track #243042 and hub stroke #2f3b4f are not in tokens.css. -->
      <circle
        cx="28"
        cy="28"
        r="22"
        fill="none"
        stroke="#243042"
        stroke-width={ringW}
        stroke-linecap="round"
        stroke-dasharray="103.67 138.23"
        transform="rotate(135 28 28)"
      ></circle>
      <circle
        cx="28"
        cy="28"
        r="22"
        fill="none"
        stroke={color}
        stroke-width={ringW}
        stroke-linecap="round"
        stroke-dasharray={dashOf(n)}
        transform="rotate(135 28 28)"
      ></circle>
      <circle cx="28" cy="28" r={hubR} fill="var(--u4-io-tile)" stroke="#2f3b4f" stroke-width="1"
      ></circle>
      <line
        x1="28"
        y1="28"
        x2="28"
        y2={tipY}
        stroke="var(--u4-text-2)"
        stroke-width={ptrW}
        stroke-linecap="round"
        transform={rotOf(n)}
      ></line>
    </svg>
  </button>
  <span style="font-size:12px;color:var(--u4-text-6)">{label}</span>
  <span style="font-size:{vSize}px;color:var(--u4-text-1);white-space:nowrap">{text}</span>
</div>

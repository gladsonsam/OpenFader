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
    fmtF,
    fmtG,
    knobDash as dashOf,
    knobRot as rotOf,
  } from '../../fixtures/designData.ts';
  import { eqParamFromDrag } from '../../state/uiLogic.ts';
  import { tap } from '../feel.ts';
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
    range,
    specOverride,
    choices,
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
    range?: import('../../fixtures/designData.ts').EqRange | null;
    specOverride?: import('../../fixtures/designData.ts').KnobSpec;
    choices?: number[];
    oninput?: ((v: number) => void) | null;
  } = $props();

  const s = $derived(specOverride ?? spec(specKey));
  const limits = $derived(range ?? (eqParam === 'f' ? [20, eqBand === 0 ? 1000 : 20000] : eqParam === 'g' ? [-15, 15] : [0.3, 16]));
  const bound = (v: number): number => clamp(v, limits[0], limits[1]);

  /** Normalised 0..1 position, per mode (design's knobItem norms). */
  function normOf(v: number): number {
    if (mode === 'norm') return clamp(v, 0, 1);
    if (mode === 'eq') {
      if (eqParam === 'g') return (v - limits[0]) / (limits[1] - limits[0]);
      return Math.log(v / limits[0]) / Math.log(limits[1] / limits[0]);
    }
    return toN(s, v);
  }
  function valueOfNorm(nd: number): number {
    const nrm = clamp(nd, 0, 1);
    if (mode === 'norm') return Math.round(nrm * 1000) / 1000;
    if (mode === 'eq') {
      const v = eqParam === 'g' ? limits[0] + (limits[1] - limits[0]) * nrm : limits[0] * Math.pow(limits[1] / limits[0], nrm);
      return bound(eqParam === 'f' ? v : Math.round(v * (eqParam === 'g' ? 10 : 100)) / (eqParam === 'g' ? 10 : 100));
    }
    const raw = fromN(s, nrm);
    return choices?.length ? choices.reduce((best, v) => Math.abs(v - raw) < Math.abs(best - raw) ? v : best) : clamp(raw, s.min, s.max);
  }
  function fmtOf(v: number): string {
    if (!Number.isFinite(v) || (mode === 'eq' && range === null)) return '—';
    if (mode === 'norm') return (fmt ?? ((x: number) => Math.round(x * 100) + '%'))(v);
    if (mode === 'eq') {
      if (eqParam === 'f') return fmtF(v);
      if (eqParam === 'g') return fmtG(v);
      return v.toFixed(2);
    }
    return s.fmt(v);
  }

  const n = $derived(Number.isFinite(value) ? clamp(normOf(value), 0, 1) : 0);
  const text = $derived(fmtOf(value));
  const aria = $derived(label + ' ' + text);
  const vSize = $derived(valueSize ?? (size >= 92 ? 15 : size <= 58 ? 12 : 13));
  const ringW = $derived(fx ? 4 : 5);
  const hubR = $derived(fx ? 16 : 15);
  const tipY = $derived(fx ? 14 : 15);
  const ptrW = $derived(fx ? 2.2 : 2.5);

  let drag: { y: number; startN: number; startV: number } | null = null;
  let lastTap = 0;

  function down(e: PointerEvent): void {
    if (disabled || !oninput || !Number.isFinite(value)) return;
    try {
      if (e.pointerType !== 'touch') (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    } catch {
      /* noop */
    }
    drag = { y: e.clientY, startN: clamp(normOf(value), 0, 1), startV: value };
  }
  function move(e: PointerEvent): void {
    if (!drag || disabled || !oninput) return;
    const dy = drag.y - e.clientY;
    if (mode === 'eq') oninput(eqParamFromDrag(eqParam, eqBand, drag.startV, dy, fine, range));
    else if (mode === 'norm')
      oninput(valueOfNorm(drag.startN + (dy / 180) * (fine ? 0.25 : 1)));
    else oninput(valueOfNorm(drag.startN + (dy / 180) * (fine ? 0.25 : 1)));
  }
  function up(): void {
    drag = null;
  }
  function reset(): void {
    if (disabled || !oninput || !Number.isFinite(value)) return;
    tap();
    if (mode === 'eq') {
      if (eqParam === 'f') oninput(bound(eqBand === 0 ? 80 : EQ_RESET_F[eqBand] ?? defaultValue));
      else if (eqParam === 'g') oninput(bound(0));
      else oninput(bound(EQ_RESET_Q[eqBand] ?? 1));
      return;
    }
    oninput(mode === 'spec' ? valueOfNorm(normOf(defaultValue)) : defaultValue);
  }
  function ontap(): void {
    const now = Date.now();
    if (now - lastTap < 350) reset();
    lastTap = now;
  }
  function key(e: KeyboardEvent): void {
    if (disabled || !oninput || !Number.isFinite(value)) return;
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
        oninput(e.key === 'Home' ? limits[0] : limits[1]);
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
  $effect(() => {
    const cancel = (): void => { drag = null; };
    window.addEventListener('openmix-control-epoch', cancel);
    window.addEventListener('openmix-control-released', cancel);
    return () => { window.removeEventListener('openmix-control-epoch', cancel); window.removeEventListener('openmix-control-released', cancel); };
  });
</script>

<div
  style="display:flex;flex-direction:column;align-items:center;gap:1px;min-width:0;opacity:{disabled
    ? 0.45
    : 1}"
>
  <button
    role="slider"
    aria-label={aria}
    aria-valuemin={mode === 'eq' ? limits[0] : mode === 'spec' ? s.min : 0}
    aria-valuemax={mode === 'eq' ? limits[1] : mode === 'spec' ? s.max : 100}
    aria-valuenow={Number.isFinite(value) ? mode === 'norm' ? Math.round(n * 100) : value : undefined}
    aria-valuetext={text}
    aria-disabled={disabled || !Number.isFinite(value) || !oninput ? 'true' : undefined}
    disabled={disabled || !Number.isFinite(value) || !oninput ? true : undefined}
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
    <svg style="pointer-events:none" width={size} height={size} viewBox="0 0 56 56" aria-hidden="true">
      <!-- Knob track + hub stroke come from theme tokens (knob-track, knob-hubline). -->
      <circle
        cx="28"
        cy="28"
        r="22"
        fill="none"
        stroke="var(--u4-knob-track)"
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
      <circle cx="28" cy="28" r={hubR} fill="var(--u4-io-tile)" stroke="var(--u4-knob-hubline)" stroke-width="1"
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

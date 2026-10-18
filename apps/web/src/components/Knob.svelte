<script lang="ts">
  // Small rotary control: vertical-drag + keyboard, engineering-unit values.
  // Used for pan / sends / phones — never for faders (those stay Strip).
  //
  // Contract: `value` is an engineering value in [min, max] owned by the
  // parent. Drags stream through onInput (parent stages locally); release
  // and keyboard commits go through onCommit (parent pushes to the backend).

  interface Props {
    label: string;
    value: number;
    min: number;
    max: number;
    step?: number;
    /** Formatted readout under the knob (dB, L/C/R, %). */
    readout?: string;
    size?: 'sm' | 'md' | 'lg';
    disabled?: boolean;
    onInput: (v: number) => void;
    onCommit?: (v: number) => void;
  }

  let {
    label,
    value,
    min,
    max,
    step = 1,
    readout = '',
    size = 'md',
    disabled = false,
    onInput,
    onCommit,
  }: Props = $props();

  const span = $derived(max - min);
  const t = $derived(span <= 0 ? 0 : Math.min(1, Math.max(0, (value - min) / span)));
  const angle = $derived(-135 + t * 270);
  const dec = $derived((String(step).split('.')[1]?.length ?? 0));

  let track: HTMLDivElement | null = $state(null);
  let startY = 0;
  let startVal = 0;
  let dragging: boolean = $state(false);

  const clamp = (v: number): number => Math.min(max, Math.max(min, v));
  const quant = (v: number): number => Number((Math.round(v / step) * step).toFixed(dec + 1));

  function onPointerDown(e: PointerEvent): void {
    if (disabled || !track) return;
    track.setPointerCapture(e.pointerId);
    dragging = true;
    startY = e.clientY;
    startVal = value;
  }

  function onPointerMove(e: PointerEvent): void {
    if (!dragging || disabled) return;
    if (e.buttons === 0) {
      dragging = false;
      return;
    }
    const dv = ((startY - e.clientY) / 160) * span;
    onInput(quant(clamp(startVal + dv)));
  }

  function endDrag(): void {
    if (!dragging) return;
    dragging = false;
    onCommit?.(value);
  }

  function onKey(e: KeyboardEvent): void {
    if (disabled) return;
    const big = e.shiftKey ? step * 10 : step;
    if (e.key === 'ArrowUp' || e.key === 'ArrowRight') {
      const v = quant(clamp(value + big));
      onInput(v);
      onCommit?.(v);
      e.preventDefault();
    } else if (e.key === 'ArrowDown' || e.key === 'ArrowLeft') {
      const v = quant(clamp(value - big));
      onInput(v);
      onCommit?.(v);
      e.preventDefault();
    } else if (e.key === 'PageUp') {
      const v = quant(clamp(value + step * 10));
      onInput(v);
      onCommit?.(v);
      e.preventDefault();
    } else if (e.key === 'PageDown') {
      const v = quant(clamp(value - step * 10));
      onInput(v);
      onCommit?.(v);
      e.preventDefault();
    } else if (e.key === 'Home') {
      onInput(max);
      onCommit?.(max);
      e.preventDefault();
    } else if (e.key === 'End') {
      onInput(min);
      onCommit?.(min);
      e.preventDefault();
    }
  }
</script>

<div class="knob" data-size={size} class:dis={disabled}>
  <div
    class="kring"
    class:grab={dragging}
    bind:this={track}
    role="slider"
    tabindex={disabled ? -1 : 0}
    aria-label={label}
    aria-valuemin={min}
    aria-valuemax={max}
    aria-valuenow={Number(value.toFixed(3))}
    aria-valuetext={readout || `${value}`}
    aria-orientation="vertical"
    aria-disabled={disabled}
    onpointerdown={onPointerDown}
    onpointermove={onPointerMove}
    onpointerup={endDrag}
    onpointercancel={endDrag}
    onkeydown={onKey}
    title={`${label} — drag vertically or use arrow keys`}
  >
    <div class="kptr" style={`transform: rotate(${angle}deg)`}></div>
  </div>
  <div class="klabel">{label}</div>
  <div class="kval">{readout}</div>
</div>

<style>
  .knob {
    --kd: 60px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 1px;
    min-width: 56px;
  }
  .knob[data-size='sm'] { --kd: 44px; min-width: 48px; }
  .knob[data-size='lg'] { --kd: 84px; min-width: 76px; }
  .knob.dis { opacity: 0.45; }
  .kring {
    width: var(--kd);
    height: var(--kd);
    border-radius: 50%;
    background: radial-gradient(circle at 35% 30%, #2a3240, #0d1117 72%);
    border: 1px solid var(--line2);
    position: relative;
    cursor: ns-resize;
    touch-action: none;
    box-shadow: var(--shadow-recess), 0 2px 4px rgba(0,0,0,0.5);
  }
  .kring.grab { border-color: var(--acc); box-shadow: 0 0 14px var(--acc), inset 0 0 6px rgba(63,169,245,0.25); }
  .kptr {
    position: absolute;
    inset: 4px;
    border-radius: 50%;
  }
  .kptr::before {
    content: '';
    position: absolute;
    left: calc(50% - 1.5px);
    top: 2px;
    width: 3px;
    height: calc(var(--kd) / 2 - 10px);
    background: var(--acc);
    border-radius: 2px;
    box-shadow: 0 0 6px var(--acc);
  }
  .klabel { font-size: 10px; font-weight: 800; color: var(--dim); text-transform: uppercase; letter-spacing: 0.4px; }
  .kval { font-size: 11px; color: var(--txt); min-height: 15px; font-variant-numeric: tabular-nums; }
</style>

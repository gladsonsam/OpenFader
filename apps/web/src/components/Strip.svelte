<script lang="ts">
  import FatStrip from './FatStrip.svelte';
  import type { EqBand } from '../lib/eqCurve';
  import { faderDb } from '../lib/format';
  import type { Strip } from '../lib/backend';

  // Fat-channel thumbnail: rendered when the parent passes per-strip EQ
  // state (`bands`) + live GR (`grDb`). Detail owns the selected channel's
  // band copy today (resets per channel — the backend never echoes param
  // state); per-strip copies for the mixer need a ch->bands store fed by
  // future relay param echo, at which point App passes
  // `bands={eqBandsFor(c.s.ch)} grDb={grFor(c.s.ch)}` at each <Strip> call
  // site. Until then both stay null and the strip renders exactly as
  // before (no layout change).

  interface Props {
    strip: Strip;
    selected?: boolean;
    dimmed?: boolean;
    soloed?: boolean;
    /** SoF/staging overrides: fader position + readout shown instead of fader. */
    displayValue?: number | null;
    displayDb?: string | null;
    /** Destination color while in sends-on-fader (fader takes its color). */
    sofColor?: string | null;
    /** Latched clip (cleared by tapping the meter). */
    clip?: boolean;
    /** Fine-adjust: drags move at 20% rate + smaller keyboard steps. */
    fine?: boolean;
    /** Mute-enable master is off: mutes are locked out. */
    muteLocked?: boolean;
    /** Layout edit mode: show reorder/gap/remove handles. */
    editMode?: boolean;
    canRemove?: boolean;
    /** Per-strip EQ copy for the fat thumbnail (null = not yet available). */
    bands?: EqBand[] | null;
    /** Per-strip gain reduction dB (>= 0, null = no GR feed yet). */
    grDb?: number | null;
    onMove?: (dir: -1 | 1) => void;
    onGap?: () => void;
    onRemove?: () => void;
    registerMeter: (ch: number, cv: HTMLCanvasElement) => void;
    unregisterMeter: (ch: number) => void;
    onFader: (v: number) => void;
    onMute: () => void;
    onSolo: () => void;
    onSelect: () => void;
    onValueTap: () => void;
    onMeterTap: () => void;
  }

  let {
    strip,
    selected = false,
    dimmed = false,
    soloed = false,
    displayValue = null,
    displayDb = null,
    sofColor = null,
    clip = false,
    fine = false,
    muteLocked = false,
    editMode = false,
    canRemove = false,
    bands = null,
    grDb = null,
    onMove,
    onGap,
    onRemove,
    registerMeter,
    unregisterMeter,
    onFader,
    onMute,
    onSolo,
    onSelect,
    onValueTap,
    onMeterTap,
  }: Props = $props();

  let track: HTMLDivElement | null = $state(null);
  let canvas: HTMLCanvasElement | null = $state(null);
  // Grow & Glow (Mackie): the grabbed fader enlarges + glows with a live value.
  let grabbed: boolean = $state(false);
  let startRaw: number = $state(0);
  let startVal: number = $state(0);

  $effect(() => {
    if (canvas) {
      registerMeter(strip.ch, canvas);
      return () => unregisterMeter(strip.ch);
    }
  });

  const shown = $derived(displayValue ?? strip.fader);
  const pct = $derived(Math.round(shown * 100));
  const readout = $derived(displayDb ?? faderDb(strip.fader));

  function rawFromEvent(e: PointerEvent): number {
    if (!track) return shown;
    const r = track.getBoundingClientRect();
    return Math.min(1, Math.max(0, 1 - (e.clientY - r.top) / r.height));
  }

  function onPointerDown(e: PointerEvent): void {
    if (!track) return;
    track.setPointerCapture(e.pointerId);
    onSelect();
    grabbed = true;
    startRaw = rawFromEvent(e);
    startVal = shown;
    onFader(startVal);
  }

  function onPointerMove(e: PointerEvent): void {
    // With pointer capture set, buttons>0 means an active drag.
    if (!e.buttons) return;
    const raw = rawFromEvent(e);
    onFader(fine ? startVal + (raw - startRaw) * 0.2 : raw);
  }

  function onPointerUp(): void {
    grabbed = false;
  }
</script>

<div
  class="strip"
  class:sel={selected}
  class:solo-dim={dimmed}
  class:grab={grabbed}
  class:clipped={clip}
  style={sofColor ? `--sof: ${sofColor}` : ''}
>
  <button class="scribble" onclick={onSelect} title="tap: select / detail">
    {strip.name}<br /><small>ch{strip.ch < 0 ? 'M' : strip.ch + 1}</small>
  </button>
  {#if editMode}
    <div class="ehandles" aria-label="layout edit">
      <button title="move left" onclick={() => onMove?.(-1)}>‹</button>
      <button title="insert blank spacer after" onclick={() => onGap?.()}>+gap</button>
      <button title="move right" onclick={() => onMove?.(1)}>›</button>
      {#if canRemove}
        <button title="remove from layer" onclick={() => onRemove?.()}>×</button>
      {/if}
    </div>
  {/if}
  <div class="mid">
    <canvas
      class="meter"
      class:clip={clip}
      width="16"
      height="190"
      bind:this={canvas}
      onclick={(e) => { e.stopPropagation(); onMeterTap(); }}
      title={clip ? 'CLIP latched — tap meter to clear' : 'staging meter (input behind, output front) — tap clears clip'}
    ></canvas>
    <div
      class="ftrack"
      class:sof={sofColor !== null}
      bind:this={track}
      onpointerdown={onPointerDown}
      onpointermove={onPointerMove}
      onpointerup={onPointerUp}
      onpointercancel={onPointerUp}
      role="slider"
      aria-label="{strip.name} fader"
      aria-valuemin="0"
      aria-valuemax="100"
      aria-valuenow={pct}
      tabindex="0"
    >
      <div class="ffill" style="height: {pct}%"></div>
      <div class="fhandle" style="bottom: calc({pct}% - 7px)"></div>
      {#if grabbed}
        <div class="fval">{readout}</div>
      {/if}
    </div>
  </div>
  <button class="db" onclick={onValueTap} title="tap: type exact value">{readout}</button>
  {#if bands}
    <div class="thumb">
      <FatStrip {bands} {grDb} compact />
    </div>
  {/if}
  {#if clip}
    <div class="clipled" role="alert">CLIP</div>
  {/if}
  <div class="mrow">
    <button
      class="mute"
      class:lit={strip.muted}
      class:locked={muteLocked}
      disabled={muteLocked}
      onclick={(e) => { e.stopPropagation(); onMute(); }}
      title={muteLocked ? 'mutes disabled (MUTE EN off)' : 'mute'}
    >M</button>
    <button class="solo" class:lit={soloed} onclick={(e) => { e.stopPropagation(); onSolo(); }}>S</button>
  </div>
</div>

<style>
  /* Strip layout inherits the gig-theme classes from app.css. */
  .scribble { cursor: pointer; font: inherit; }
  /* Fat thumbnail slot: full strip width, zero footprint until fed. */
  .thumb { width: 100%; }
</style>

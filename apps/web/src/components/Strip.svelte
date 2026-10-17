<script lang="ts">
  import FatStrip from './FatStrip.svelte';
  import type { EqBand } from '../lib/eqCurve';
  import { faderDb } from '../lib/format';
  import { ensurePerfOverlay, setMeterVisible, sizeMeterCanvas } from '../lib/meters';
  import type { Strip } from '../lib/backend';

  // Fat-channel thumbnail: rendered when the parent passes per-strip EQ
  // state (`bands`) + live GR (`grDb`). App feeds both from the ch->bands
  // store (lib/bands.ts — local-edit mirror, the relay never echoes param
  // state) at each <Strip> call site: `bands={eqBandsFor(ch)} grDb={grFor(ch, gr)}`.
  // Both stay null for MAIN, which renders exactly as before (no layout change).

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
      const cv = canvas;
      registerMeter(strip.ch, cv);
      // Backing store sized once (DPR-capped); CSS keeps 16x190 display size.
      sizeMeterCanvas(cv, 16, 190);
      ensurePerfOverlay();
      // Skip painting while scrolled out of view; observer fires only on change.
      setMeterVisible(cv, true);
      const io = new IntersectionObserver((entries) => {
        for (let i = 0; i < entries.length; i++) setMeterVisible(cv, entries[i].isIntersecting);
      });
      io.observe(cv);
      return () => {
        io.disconnect();
        setMeterVisible(cv, true);
        unregisterMeter(strip.ch);
      };
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

  function faderKey(e: KeyboardEvent): void {
    const step = fine ? 0.002 : e.shiftKey ? 0.05 : 0.01;
    if (e.key === 'ArrowUp' || e.key === 'ArrowRight') {
      onFader(clamp01(shown + step));
      e.preventDefault();
    } else if (e.key === 'ArrowDown' || e.key === 'ArrowLeft') {
      onFader(clamp01(shown - step));
      e.preventDefault();
    } else if (e.key === 'Home') {
      onFader(1);
      e.preventDefault();
    } else if (e.key === 'End') {
      onFader(0);
      e.preventDefault();
    } else if (e.key === 'PageUp') {
      onFader(clamp01(shown + step * 10));
      e.preventDefault();
    } else if (e.key === 'PageDown') {
      onFader(clamp01(shown - step * 10));
      e.preventDefault();
    }
  }

  const clamp01 = (v: number): number => Math.min(1, Math.max(0, v));

  function meterKey(e: KeyboardEvent): void {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      onMeterTap();
    }
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
  <button class="scribble" aria-pressed={selected} aria-label={`select ${strip.name} for detail`} onclick={onSelect} title="tap: select / detail">
    {strip.name}<br /><small>ch{strip.ch < 0 ? 'M' : strip.ch + 1}</small>
  </button>
  {#if editMode}
    <div class="ehandles" role="group" aria-label="layout edit">
      <button title="move left" aria-label="move strip left" onclick={() => onMove?.(-1)}>‹</button>
      <button title="insert blank spacer after" aria-label="insert blank spacer after" onclick={() => onGap?.()}>+gap</button>
      <button title="move right" aria-label="move strip right" onclick={() => onMove?.(1)}>›</button>
      {#if canRemove}
        <button title="remove from layer" aria-label="remove strip from layer" onclick={() => onRemove?.()}>×</button>
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
      tabindex="0"
      role="button"
      aria-label={clip ? `CLIP latched on ${strip.name} — activate to clear` : `staging meter for ${strip.name} — activate clears clip`}
      onclick={(e) => { e.stopPropagation(); onMeterTap(); }}
      onkeydown={meterKey}
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
      aria-valuetext={readout}
      aria-orientation="vertical"
      onkeydown={faderKey}
      tabindex="0"
    >
      <div class="ffill" style="height: {pct}%"></div>
      <div class="fhandle" style="bottom: calc({pct}% - 7px)"></div>
      {#if grabbed}
        <div class="fval">{readout}</div>
      {/if}
    </div>
  </div>
  <button class="db" aria-label={`type exact value for ${strip.name}, currently ${readout}`} onclick={onValueTap} title="tap: type exact value">{readout}</button>
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
      aria-pressed={strip.muted}
      aria-label={muteLocked ? `mute ${strip.name} (locked — MUTE EN off)` : strip.muted ? `unmute ${strip.name}` : `mute ${strip.name}`}
      onclick={(e) => { e.stopPropagation(); onMute(); }}
      title={muteLocked ? 'mutes disabled (MUTE EN off)' : 'mute'}
    >{strip.muted ? 'M✓' : 'M'}</button>
    <button
      class="solo"
      class:lit={soloed}
      aria-pressed={soloed}
      aria-label={soloed ? `unsolo ${strip.name}` : `solo ${strip.name}`}
      onclick={(e) => { e.stopPropagation(); onSolo(); }}
      title="solo (local monitor)"
    >{soloed ? 'S✓' : 'S'}</button>
  </div>
</div>

<style>
  /* Strip layout inherits the gig-theme classes from app.css. */
  .scribble { cursor: pointer; font: inherit; }
  /* Fat thumbnail slot: full strip width, zero footprint until fed. */
  .thumb { width: 100%; }
</style>

<script>
  import { faderDb } from '../lib/format.js';

  let {
    strip,
    selected = false,
    dimmed = false,
    soloed = false,
    registerMeter,
    unregisterMeter,
    onFader,
    onMute,
    onSolo,
    onSelect,
  } = $props();

  let track = $state(null);
  let canvas = $state(null);

  $effect(() => {
    if (canvas) {
      registerMeter(strip.ch, canvas);
      return () => unregisterMeter(strip.ch);
    }
  });

  const pct = $derived(Math.round(strip.fader * 100));

  function setFromEvent(e) {
    const r = track.getBoundingClientRect();
    const v = Math.min(1, Math.max(0, 1 - (e.clientY - r.top) / r.height));
    onFader(v);
  }

  function onPointerDown(e) {
    track.setPointerCapture(e.pointerId);
    onSelect();
    setFromEvent(e);
  }

  function onPointerMove(e) {
    // With pointer capture set, buttons>0 means an active drag.
    if (e.buttons) setFromEvent(e);
  }
</script>

<div class="strip" class:sel={selected} class:solo-dim={dimmed}>
  <button class="scribble" onclick={onSelect} title="tap: select / detail">
    {strip.name}<br /><small>ch{strip.ch < 0 ? 'M' : strip.ch + 1}</small>
  </button>
  <div class="mid">
    <canvas class="meter" width="16" height="190" bind:this={canvas}></canvas>
    <div
      class="ftrack"
      bind:this={track}
      onpointerdown={onPointerDown}
      onpointermove={onPointerMove}
      role="slider"
      aria-label="{strip.name} fader"
      aria-valuemin="0"
      aria-valuemax="100"
      aria-valuenow={pct}
      tabindex="0"
    >
      <div class="ffill" style="height: {pct}%"></div>
      <div class="fhandle" style="bottom: calc({pct}% - 7px)"></div>
    </div>
  </div>
  <div class="db">{faderDb(strip.fader)}</div>
  <div class="mrow">
    <button class="mute" class:lit={strip.muted} onclick={(e) => { e.stopPropagation(); onMute(); }}>M</button>
    <button class="solo" class:lit={soloed} onclick={(e) => { e.stopPropagation(); onSolo(); }}>S</button>
  </div>
</div>

<style>
  /* Strip layout inherits the gig-theme classes from app.css. */
  .scribble { cursor: pointer; font: inherit; }
</style>

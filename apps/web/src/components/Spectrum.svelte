<script>
  // Spectrum message contract (serve does NOT send this yet — fallback mode):
  //   relay -> {"type":"spectrum","ch":0,"bins":[...]}
  // `ch` is the 0-based channel index, `bins` an array of 0..1 magnitudes
  // (peak-normalized, low→high frequency). When `bins` is null the component
  // renders a `levels`-derived fallback (interpolated bars from the live meter
  // level) so the panel still shows movement until the relay sends spectra.
  import { onMount, onDestroy } from 'svelte';
  import { levels } from '../lib/levels.js';

  // ch: 0-based channel index. bins: number[]|null (live spectrum when set).
  let { ch = 0, bins = null } = $props();

  const NBARS = 64;
  let canvas = $state(null);
  let raf = 0;

  // Deterministic per-bar shape so the fallback reads as a spectrum that
  // decays with frequency (no randomness — same bars every render).
  const shape = Array.from({ length: NBARS }, (_, i) => {
    const pseudo = (() => {
      const x = Math.sin(i * 12.9898) * 43758.5453;
      return x - Math.floor(x);
    })();
    return Math.exp(-i / 18) * (0.55 + 0.45 * pseudo);
  });

  function frameBins() {
    if (Array.isArray(bins) && bins.length) {
      // Resample whatever the relay sent onto NBARS bars.
      const out = new Array(NBARS);
      for (let i = 0; i < NBARS; i++) {
        const src = bins[Math.min(bins.length - 1, Math.floor((i / NBARS) * bins.length))];
        out[i] = Math.min(1, Math.max(0, Number(src) || 0));
      }
      return out;
    }
    // TODO: wire ws spectrum — no `spectrum` message from serve yet, so
    // interpolate bars from the live meter level until it arrives.
    const lvl = ch >= 0 ? (levels[ch] ?? 0) : 0;
    return shape.map((s) => Math.min(1, lvl * 1.6 * s));
  }

  function paint() {
    const cv = canvas;
    if (cv) {
      const dpr = window.devicePixelRatio || 1;
      const w = cv.clientWidth * dpr;
      const h = cv.clientHeight * dpr;
      if (cv.width !== w || cv.height !== h) {
        cv.width = Math.max(1, w);
        cv.height = Math.max(1, h);
      }
      const ctx = cv.getContext('2d');
      const data = frameBins();
      const bw = cv.width / NBARS;
      ctx.clearRect(0, 0, cv.width, cv.height);
      for (let i = 0; i < NBARS; i++) {
        const v = data[i];
        const bh = Math.max(1, v * cv.height);
        const t = i / (NBARS - 1); // 0 = bass, 1 = treble
        ctx.fillStyle = t < 0.55 ? '#4ade80' : t < 0.8 ? '#facc15' : '#f87171';
        ctx.fillRect(i * bw + 0.5, cv.height - bh, bw - 1, bh);
      }
    }
    raf = requestAnimationFrame(paint);
  }

  onMount(() => {
    raf = requestAnimationFrame(paint);
  });

  onDestroy(() => {
    cancelAnimationFrame(raf);
  });
</script>

<div class="spec" class:live={Array.isArray(bins) && bins.length} title={Array.isArray(bins) && bins.length ? 'live spectrum' : 'levels fallback — no spectrum message yet'}>
  <canvas bind:this={canvas}></canvas>
  {#if !(Array.isArray(bins) && bins.length)}
    <span class="tag">fallback</span>
  {/if}
</div>

<style>
  .spec {
    position: relative;
    height: 72px;
    background: #0b0f14;
    border-radius: 6px;
    overflow: hidden;
  }
  .spec canvas {
    width: 100%;
    height: 100%;
    display: block;
  }
  .tag {
    position: absolute;
    right: 4px;
    top: 2px;
    font-size: 10px;
    opacity: 0.55;
    color: #9aa4b2;
  }
</style>

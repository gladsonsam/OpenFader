<!-- Signal meter, pixel-matched to the design's meter columns.
     Drawn on canvas (imperative rAF, no reactive hot path): gradient fill
     (green 0-66%, amber 66-84%, red 84-100%), dark cover above the fill
     line, 2px peak-hold line, clip block, and (strip only) the GR block.
     Variants: 'strip' (6px column + 5px clip/GR column), 'aside' (8px
     column + 6px clip), 'master' (dual 6px columns), 'hbar' (the Input
     tab horizontal level bar, full-width 12px). Levels are read through
     getters every frame; the 1.5 s peak/clip hold comes from the
     MeterSim driving the getters (fixtures/meterSim constants
     PEAK_HOLD_MS / CLIP_HOLD_MS). -->
<script lang="ts">
  import { dbToPos } from '../../fixtures/designData.ts';
  import { isAppHidden } from '../../../mobile.ts';

  export interface MeterSource {
    getLevel: (ch: number) => number;
    getPeak?: (ch: number) => number | null;
    getClip?: (ch: number) => boolean;
    /** Strip GR block: lit when the comp is reducing (design: comp on && lvl > thr). */
    grOn?: boolean | (() => boolean);
    /** Draw the peak line (design's peakHold display flag). */
    showPeak?: boolean;
  }

  let {
    variant = 'strip',
    source,
    frozen = false,
  }: {
    variant?: 'strip' | 'aside' | 'master' | 'hbar';
    source: MeterSource;
    /** When true, draw once and stop the rAF loop (fixture screenshots). */
    frozen?: boolean;
  } = $props();

  let root: HTMLDivElement | null = $state(null);
  let canvas: HTMLCanvasElement | null = $state(null);

  /** Colours resolve from ui/tokens.css at runtime, literals = design values. */
  function cssVar(name: string, fallback: string): string {
    try {
      const v = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
      return v || fallback;
    } catch {
      return fallback;
    }
  }

  interface Palette {
    green: string;
    amber: string;
    red: string;
    cover: string;
    peak: string;
    clipOn: string;
    clipOff: string;
    grOn: string;
    grOff: string;
  }

  /** Resolved once per mount (plus theme changes), not per frame. */
  function resolvePalette(): Palette {
    return {
      green: cssVar('--u4-meter-green', '#5fe060'),
      amber: cssVar('--u4-meter-amber', '#e7c12f'),
      red: cssVar('--u4-red', '#ff4a3d'),
      cover: cssVar('--u4-meter-cover', '#1c2416'),
      // TODO token: peak line #f2f5f9 is not in tokens.css.
      peak: '#f2f5f9',
      clipOn: cssVar('--u4-clip', '#ff3b30'),
      clipOff: cssVar('--u4-clip-dark', '#3a1214'),
      grOn: cssVar('--u4-gr-lit', '#ff6b4a'),
      grOff: cssVar('--u4-gr-dark', '#3a1a14'),
    };
  }

  let pal: Palette = $state({
    green: '#5fe060',
    amber: '#e7c12f',
    red: '#ff4a3d',
    cover: '#1c2416',
    peak: '#f2f5f9',
    clipOn: '#ff3b30',
    clipOff: '#3a1214',
    grOn: '#ff6b4a',
    grOff: '#3a1a14',
  });

  function draw(pal: Palette): void {
    const cv = canvas;
    if (!cv) return;
    // Off-screen pager cells (clientWidth == 0) skip all canvas work but
    // keep the loop alive so they resume when paged back in.
    const w = cv.clientWidth;
    const h = cv.clientHeight;
    if (!w || !h) return;
    const dpr = Math.min(2, window.devicePixelRatio || 1);
    const bw = Math.max(1, Math.round(w * dpr));
    const bh = Math.max(1, Math.round(h * dpr));
    if (cv.width !== bw || cv.height !== bh) {
      cv.width = bw;
      cv.height = bh;
    }
    const g = cv.getContext('2d');
    if (!g) return;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    const showPeak = source.showPeak !== false;
    if (variant === 'hbar') {
      drawHBar(g, w, h, pal);
      return;
    }
    // Vertical column layout per variant (design insets, px):
    //   strip:  meter x0 w6,  clip/gr x8 w5
    //   aside:  meter x0 w8,  clip x12 w6
    //   master: meter0 x0 w6, meter1 x9 w6
    if (variant === 'master') {
      drawColumn(g, 0, 6, h, 0, pal, showPeak);
      drawColumn(g, 9, 6, h, 1, pal, showPeak);
      return;
    }
    const mw = variant === 'aside' ? 8 : 6;
    drawColumn(g, 0, mw, h, 0, pal, showPeak);
    const clip = source.getClip ? source.getClip(0) : false;
    // Strip canvas is 14px at right:1; clip/gr sit at right:1 w5 -> x9 w5.
    // Aside canvas is 18px at right:0; clip sits at right:0 w6 -> x12 w6.
    const cx = variant === 'aside' ? 12 : 9;
    const cw = variant === 'aside' ? 6 : 5;
    g.fillStyle = clip ? pal.clipOn : pal.clipOff;
    roundRect(g, cx, 0, cw, h * 0.22, 1);
    g.fill();
    if (variant === 'strip') {
      const gr = typeof source.grOn === 'function' ? source.grOn() : !!source.grOn;
      g.fillStyle = gr ? pal.grOn : pal.grOff;
      roundRect(g, cx, h * 0.26, cw, h * 0.18, 1);
      g.fill();
    }
  }

  function drawColumn(
    g: CanvasRenderingContext2D,
    x: number,
    w: number,
    h: number,
    ch: number,
    pal: Palette,
    showPeak: boolean,
  ): void {
    const lvl = source.getLevel(ch);
    const mpos = Math.max(0, Math.min(1, dbToPos(lvl)));
    // Gradient bands measured from the bottom (design: to top, 66/84).
    const yGreen = h * (1 - 0.66);
    const yAmber = h * (1 - 0.84);
    g.fillStyle = pal.green;
    g.fillRect(x, yGreen, w, h - yGreen);
    g.fillStyle = pal.amber;
    g.fillRect(x, yAmber, w, yGreen - yAmber);
    g.fillStyle = pal.red;
    g.fillRect(x, 0, w, yAmber);
    // Dark cover above the fill line.
    const fillY = h * (1 - mpos);
    if (fillY > 0) {
      g.fillStyle = pal.cover;
      g.fillRect(x, 0, w, fillY);
    }
    // Peak-hold line (2px), hidden below -80 dB like the design.
    const pk = source.getPeak ? source.getPeak(ch) : null;
    if (showPeak && pk != null && pk > -80) {
      const py = Math.max(0, Math.min(h - 2, h * (1 - Math.max(0, Math.min(1, dbToPos(pk))))));
      g.fillStyle = pal.peak;
      g.fillRect(x, py, w, 2);
    }
  }

  function drawHBar(
    g: CanvasRenderingContext2D,
    w: number,
    h: number,
    pal: Palette,
  ): void {
    const lvl = source.getLevel(0);
    // Design: gradient to the right, 70/88; cover from the right.
    g.fillStyle = pal.green;
    g.fillRect(0, 0, w * 0.7, h);
    g.fillStyle = pal.amber;
    g.fillRect(w * 0.7, 0, w * 0.18, h);
    g.fillStyle = pal.red;
    g.fillRect(w * 0.88, 0, w - w * 0.88, h);
    const frac = Math.max(0, Math.min(1, (lvl + 60) / 60));
    const coverW = (1 - frac) * w;
    if (coverW > 0) {
      g.fillStyle = pal.cover;
      g.fillRect(w - coverW, 0, coverW, h);
    }
  }

  function roundRect(
    g: CanvasRenderingContext2D,
    x: number,
    y: number,
    w: number,
    h: number,
    r: number,
  ): void {
    g.beginPath();
    g.roundRect(x, y, w, h, r);
  }

  $effect(() => {
    const update = (): void => {
      pal = resolvePalette();
    };
    update();
    const mo = new MutationObserver(update);
    try {
      mo.observe(document.documentElement, {
        attributes: true,
        attributeFilter: ['class', 'style'],
      });
    } catch {
      /* noop */
    }
    return () => mo.disconnect();
  });

  $effect(() => {
    draw(pal);
    if (frozen) return;
    let raf = 0;
    const loop = (): void => {
      // Background battery: skip canvas painting while hidden. The engine
      // push stream stays live (no transport change), so the next visible
      // frame paints fresh levels instantly.
      if (!isAppHidden()) draw(pal);
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(raf);
  });
</script>

{#if variant === 'hbar'}
  <div
    bind:this={root}
    style="position:relative;width:100%;height:12px;overflow:hidden;border-radius:2px"
  >
    <canvas bind:this={canvas} style="position:absolute;inset:0;width:100%;height:100%;display:block"></canvas>
  </div>
{:else if variant === 'master'}
  <div
    bind:this={root}
    style="position:absolute;right:0;width:19px;top:0;bottom:0;display:block"
  >
    <canvas bind:this={canvas} style="position:absolute;inset:0;width:100%;height:100%;display:block"></canvas>
  </div>
{:else if variant === 'aside'}
  <div
    bind:this={root}
    style="position:absolute;right:0;width:18px;top:0;bottom:0;display:block"
  >
    <canvas bind:this={canvas} style="position:absolute;inset:0;width:100%;height:100%;display:block"></canvas>
  </div>
{:else}
  <div
    bind:this={root}
    style="position:absolute;right:1px;width:14px;top:0;bottom:0;display:block"
  >
    <canvas bind:this={canvas} style="position:absolute;inset:0;width:100%;height:100%;display:block"></canvas>
  </div>
{/if}

<!-- One meter-bridge group's ladder canvas (mixer control software pattern).

     Imperative canvas repaint on rAF reading the live engine arrays directly
     (SPEC 2a: meter frames never touch reactive state — same pattern as the
     mixer meter loop and the channel RTA graph). Scribbles render as HTML
     below (crisp text, accessible); only the ladders are canvas. -->
<script lang="ts">
  import type { EngineWorld } from '../../world/engineWorld/engineWorld.ts';
  import { isAppHidden } from '../../../mobile.ts';
  import {
    BRIDGE_FLOOR_DB,
    liveClipOn,
    liveLevelDb,
    livePeakDb,
    segColor,
    type BridgeGroup,
  } from './meterBridgeModel.ts';

  let {
    group,
    world,
    online,
  }: {
    group: BridgeGroup;
    world: EngineWorld | null;
    online: boolean;
  } = $props();

  // Geometry (CSS px; scaled by devicePixelRatio at draw time).
  const GUTTER = 30; // scale-number gutter
  const PITCH = 46; // ladder pitch (ladder 18 + gap)
  const LADDER_W = 18;
  const LADDER_H = 168;
  const TOP_PAD = 14; // clip-LED row
  const SEG_DB = 2; // 25 segments over 0..-50
  const TICK_EVERY = 5; // scale label every 10 dB

  let canvas: HTMLCanvasElement | null = $state(null);
  const cssW = $derived(GUTTER + group.ladders.length * PITCH + 8);
  const cssH = $derived(TOP_PAD + LADDER_H + 4);

  const SEG_COLORS = { red: '#f0524d', amber: '#f0b429', green: '#3fd06a' } as const;
  const SEG_DARK = '#161d27';

  function dbToY(db: number): number {
    const f = Math.min(1, Math.max(0, (0 - db) / (0 - BRIDGE_FLOOR_DB)));
    return TOP_PAD + f * LADDER_H;
  }

  function draw(): void {
    const el = canvas;
    if (!el) return;
    const dpr = Math.min(3, window.devicePixelRatio || 1);
    const W = Math.round(cssW * dpr);
    const H = Math.round(cssH * dpr);
    if (el.width !== W || el.height !== H) {
      el.width = W;
      el.height = H;
    }
    const ctx = el.getContext('2d');
    if (!ctx) return;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, cssW, cssH);
    const now = performance.now();
    const nSeg = Math.round((0 - BRIDGE_FLOOR_DB) / SEG_DB);
    const segH = LADDER_H / nSeg;
    group.ladders.forEach((l, i) => {
      const x = GUTTER + i * PITCH + (PITCH - LADDER_W) / 2;
      const db = liveLevelDb(world, online, l.ch) ?? BRIDGE_FLOOR_DB - 1;
      const lit = db <= BRIDGE_FLOOR_DB ? 0 : Math.ceil((db - BRIDGE_FLOOR_DB) / SEG_DB);
      for (let s = 0; s < nSeg; s++) {
        const segTopDb = 0 - s * SEG_DB;
        const y = TOP_PAD + s * segH;
        const on = s < lit;
        ctx.fillStyle = on ? SEG_COLORS[segColor(segTopDb - SEG_DB / 2)] : SEG_DARK;
        ctx.fillRect(x, y + 0.5, LADDER_W, segH - 1);
      }
      // Peak-hold tick (white) where the engine has peak data.
      const pk = livePeakDb(world, online, l.ch);
      if (pk !== null && pk > BRIDGE_FLOOR_DB) {
        ctx.fillStyle = '#e8edf5';
        ctx.fillRect(x, dbToY(pk) - 1, LADDER_W, 2);
      }
      // Clip LED.
      const clip = liveClipOn(world, online, l.ch, now);
      ctx.fillStyle = clip ? '#ff3b30' : '#2a1215';
      ctx.fillRect(x + LADDER_W / 2 - 4, 2, 8, 8);
    });
    // Scale numbers (0..-50, every 10 dB).
    ctx.fillStyle = '#7f8a9c';
    ctx.font = '9px system-ui, sans-serif';
    ctx.textAlign = 'right';
    ctx.textBaseline = 'middle';
    for (let s = 0; s <= nSeg; s += TICK_EVERY) {
      const db = 0 - s * SEG_DB;
      ctx.fillText(String(db), GUTTER - 4, TOP_PAD + s * segH);
    }
  }

  $effect(() => {
    // Re-run when the ladder set changes (offline <-> online, desk change).
    void group.ladders.length;
    let raf = 0;
    const loop = (): void => {
      // Background battery: skip canvas painting while hidden (stream
      // stays live, resume is instant). rAF itself is throttled by the
      // browser; the guard covers WebViews that keep pumping frames.
      if (!isAppHidden()) draw();
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(raf);
  });
</script>

<div style="overflow-x:auto;max-width:100%">
  <!-- svelte-ignore a11y_no_interactive_element_to_noninteractive_role -->
  <canvas
    bind:this={canvas}
    style="display:block;width:{cssW}px;height:{cssH}px"
    role="img"
    aria-label="{group.title} live levels"
  ></canvas>
  <div style="display:flex;margin-left:{GUTTER}px" aria-hidden="false">
    {#each group.ladders as l (l.ch + ':' + l.sub)}
      <div
        style="flex:none;width:{PITCH}px;padding:2px 2px 0;box-sizing:border-box;text-align:center;font-size:10px;line-height:1.25;color:#aeb8c4;white-space:nowrap;overflow:hidden;text-overflow:ellipsis"
        title={l.label ?? 'no data'}
      >
        {l.label ?? '–'}
      </div>
    {/each}
  </div>
</div>

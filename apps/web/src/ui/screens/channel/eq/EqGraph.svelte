<!-- W5b `eq-graph` module: the big EQ response graph. Presentational: the
     EqGraphVm in via layout `bind`, node gestures out (`select` with the
     band, `node` with band/freq/gain in Hz/dB). SVG geometry verbatim from
     design-template.html (viewBox 0 0 1000 300, preserveAspectRatio none,
     grid strokes, dashed band fill, yellow summed curve + fill to the
     0 dB line). Imperative redraw is free: paths are recomputed only when
     the eq VM changes (no per-frame work).
     Pointer: buttons capture the pointer (design nodeDown); move maps the
     client position to freq/gain via eqPointFromPos; HPF clamps at 1000 Hz
     via eqNodeApply. Locked desks: selection still applies, value edits are
     dropped (design move() early-returns when locked); the action layer
     enforces the same rule.
     Keyboard (addition to the design's pointer behaviour): arrows nudge
     freq (semitone, quarter in fine) / gain (0.5 dB, 0.1 in fine);
     Home/End jump to range ends. Focus ring is the design's
     2px #7cc4ff outline. -->
<script lang="ts">
  import { fade } from 'svelte/transition';
  import { REDUCED_MOTION } from '../../../primitives/feel.ts';
  import { eqNodeApply, eqPointFromPos, type EqGraphVm } from './eqModel.ts';
  import type { ModuleContext } from '../../../modules/types.ts';

  let {
    props = {},
    ctx: _ctx,
    nodeId: _nodeId,
    onEvent,
  }: {
    props: { graph?: EqGraphVm | null; locked?: boolean; fine?: boolean };
    ctx: ModuleContext;
    nodeId: string;
    onEvent?: (name: string, payload?: unknown) => void;
  } = $props();

  const graph = $derived(props.graph);
  const locked = $derived(props.locked ?? false);
  const fine = $derived(props.fine ?? false);

  let box: HTMLDivElement | null = null;
  let dragBand: number | null = null;

  function down(e: PointerEvent, band: number): void {
    onEvent?.('select', { band });
    try {
      (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    } catch {
      /* noop */
    }
    dragBand = band;
  }
  function move(e: PointerEvent, band: number): void {
    if (dragBand !== band || locked || !box || !graph) return;
    const r = box.getBoundingClientRect();
    if (r.width <= 0 || r.height <= 0) return;
    const p = eqPointFromPos((e.clientX - r.left) / r.width, (e.clientY - r.top) / r.height);
    const a = eqNodeApply(band, p.f, p.g);
    const cur = graph.nodes[band];
    if (band === 0) onEvent?.('node', { band, f: a.f, g: cur ? cur.gain : -3 });
    else onEvent?.('node', { band, f: a.f, g: a.g });
  }
  function up(): void {
    dragBand = null;
  }

  function key(e: KeyboardEvent, band: number): void {
    const node = graph?.nodes[band];
    if (!node) return;
    const step = fine ? 0.25 : 1;
    let f = node.freq;
    let g = node.gain;
    if (e.key === 'ArrowLeft') f = f * Math.pow(2, -step / 12);
    else if (e.key === 'ArrowRight') f = f * Math.pow(2, step / 12);
    else if (band !== 0 && (e.key === 'ArrowUp' || e.key === 'ArrowDown')) {
      g = Math.round((g + (e.key === 'ArrowUp' ? 0.5 : -0.5) * step) * 10) / 10;
    } else if (e.key === 'Home' || e.key === 'End') {
      f = e.key === 'Home' ? 20 : band === 0 ? 1000 : 20000;
      if (band !== 0) g = e.key === 'Home' ? -15 : 15;
    } else return;
    e.preventDefault();
    onEvent?.('select', { band });
    if (locked) return;
    const a = eqNodeApply(band, f, g);
    onEvent?.('node', { band, f: a.f, g: band === 0 ? node.gain : a.g });
  }
</script>

<div
  bind:this={box}
  data-graph="1"
  in:fade={{ duration: REDUCED_MOTION ? 0 : 120 }}
  style="box-sizing:content-box;flex:1 1 280px;min-height:220px;position:relative;border-radius:6px;border:1px solid var(--u4-line-panel);background:var(--u4-well-bg);overflow:hidden;touch-action:none"
>
  {#if graph}
    <svg
      viewBox="0 0 1000 300"
      preserveAspectRatio="none"
      style="position:absolute;inset:0;width:100%;height:100%;display:block"
      aria-hidden="true"
    >
      {#each graph.gridV as gv (gv.x + gv.stroke)}
        <line x1={gv.x} y1="0" x2={gv.x} y2="300" stroke={gv.stroke} stroke-width="1" vector-effect="non-scaling-stroke"
        ></line>
      {/each}
      {#each graph.gridH as gh (gh.y + gh.stroke)}
        <line x1="0" y1={gh.y} x2="1000" y2={gh.y} stroke={gh.stroke} stroke-width="1" vector-effect="non-scaling-stroke"
        ></line>
      {/each}
      <path
        d={graph.bandFill}
        fill={graph.bandFillColor}
        stroke={graph.bandStroke}
        stroke-width="1.2"
        stroke-dasharray="4 3"
        vector-effect="non-scaling-stroke"
      ></path>
      <path d={graph.fillPath} fill={graph.curveFill} stroke="none"></path>
      <path
        d={graph.curvePath}
        fill="none"
        stroke={graph.curveStroke}
        stroke-width="2.5"
        vector-effect="non-scaling-stroke"
      ></path>
    </svg>
    {#each graph.freqLabels as l (l.label)}
      <span
        style="position:absolute;bottom:4px;left:{l.x}%;transform:translateX(-50%);font-size:12px;color:var(--u4-text-10)"
        >{l.label}</span
      >
    {/each}
    {#each graph.gainLabels as l (l.label)}
      <span
        style="position:absolute;left:6px;top:{l.y}%;transform:translateY(-50%);font-size:12px;color:var(--u4-text-10)"
        >{l.label}</span
      >
    {/each}
    {#each graph.nodes as n (n.band)}
      <button
        class="eq-node"
        aria-label={n.aria}
        aria-valuetext={n.valueText}
        onpointerdown={(e) => down(e, n.band)}
        onpointermove={(e) => move(e, n.band)}
        onpointerup={up}
        onpointercancel={up}
        onkeydown={(e) => key(e, n.band)}
        style="position:absolute;left:{n.x}%;top:{n.y}%;width:{n.size}px;height:{n.size}px;transform:translate(-50%, -50%);border-radius:50%;border:2px solid {n.ring};background:{n.color};color:#0b0f14;font-size:13px;font-weight:600;opacity:{n.op};padding:0;touch-action:none;cursor:grab;box-shadow:0 2px 8px rgba(0,0,0,.6)"
        >{n.label}</button
      >
    {/each}
  {/if}
</div>

<style>
  button.eq-node:focus-visible {
    outline: 2px solid #7cc4ff;
    outline-offset: 1px;
  }
</style>

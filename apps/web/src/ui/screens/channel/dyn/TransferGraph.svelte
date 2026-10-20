<!-- W5b `dyn-transfer-graph` module: the gate/compressor transfer-curve
     card. Presentational: precomputed geometry (via layout `bind` from the
     dynModel port of dynVals) + live params in; handle/plot drags emit raw
     spec values through `thr` / `ratio` / `makeup` for the layout `on`
     action (channel.dynKnob, lock-checked there). Verbatim from
     design-template.html (isDynTab graph card): y-axis gutter, plot box
     (grid, unity diagonal, fill, threshold line, gold curve, threshold
     tag, operating dot, Input/Output captions), x-axis strip, GR column.
     The curve/handles paint reactively on param edits; the operating dot
     paints imperatively via rAF from getLevel (SPEC 2a); frozen fixture
     mode paints the silent dot once from staticDotL/staticDotT.
     Handles (reference-hardware G/T/R circles): T rides the knee and
     drags horizontally (threshold); comp R sits at the top-right curve
     end and G at the bottom-left start, both dragging vertically (ratio /
     makeup gain). Each handle is a slider (pointer drag with immediate
     capture like the EQ nodes + arrow/PageUp/PageDown/Home/End keys). Plot-background drag
     still works as before (horizontal: threshold; comp vertical: ratio).
     Direction lock with slop: gate ignores vertical (touch-action pan-y
     scrolls the page natively); comp needs both axes, so it takes touch
     (touch-action none) and the lock decides per gesture. -->
<script lang="ts">
  import {
    dynDotNums,
    dynHandles,
    dynMakeupRaw,
    dynRatioRaw,
    dynThrRaw,
    type DynAxisTick,
    type DynGridLine,
    type DynHandle,
    type DynKind,
    type DynParams,
  } from './dynModel.ts';
  import type { ModuleContext } from '../../../modules/types.ts';
  import { clamp, fromN, spec, toN, type Comp } from '../../../fixtures/designData.ts';
  import { dynSpecKey } from '../../../state/uiLogic.ts';
  import { tap } from '../../../primitives/feel.ts';
  import GrBar from './GrBar.svelte';

  let {
    props = {},
    ctx: _ctx,
    nodeId: _nodeId,
    onEvent,
  }: {
    props: {
      kind?: DynKind;
      color?: string;
      aria?: string;
      path?: string;
      fill?: string;
      fillColor?: string;
      thrX?: string;
      thrLeft?: string;
      thrText?: string;
      grid?: DynGridLine[];
      axis?: DynAxisTick[];
      params?: DynParams | null;
      frozen?: boolean;
      staticDotL?: string;
      staticDotT?: string;
      /** Live level reader (dBFS). Ignored when frozen. */
      getLevel?: (() => number) | null;
      /** Card min-height px (layouts shrink it on phones). */
      minH?: number;
      /** Flex shorthand override (phone stacks the graph full-width). */
      flex?: string;
      /** Width override (phone: 100% to fill the stacked column). */
      width?: number | string;
      /** Right margin on the plot px (phone layouts pass 24: keeps the
       *  44px R handle inside the scrollport so the ancestor scroller
       *  never clips it or grows a horizontal scrollbar; desktop
       *  defaults to 0 and relies on the structural column padding
       *  below for the same 22px of breathing room). */
      plotMR?: number;
    };
    ctx: ModuleContext;
    nodeId: string;
    onEvent?: (name: string, payload?: unknown) => void;
  } = $props();

  const kind = $derived(props.kind ?? 'comp');
  const color = $derived(props.color ?? '#ff9f43');
  const grid = $derived(props.grid ?? []);
  const axis = $derived(props.axis ?? []);
  const params = $derived(props.params ?? null);
  // Ratio gestures solve from the comp threshold + makeup (follow-the-line
  // mapping); gate never reaches a ratio branch, so the fallback is dead.
  const compThrMakeup = $derived(
    kind === 'comp' && params
      ? { thr: (params as Comp).thr, makeup: (params as Comp).makeup }
      : { thr: -60, makeup: 0 },
  );
  const frozen = $derived(props.frozen ?? true);
  const getLevel = $derived(props.getLevel ?? null);
  const minH = $derived(props.minH ?? 300);
  const flexCss = $derived(props.flex ?? '1 1 400px');
  const widthCss = $derived(
    props.width == null ? '' : `;width:${typeof props.width === 'number' ? props.width + 'px' : props.width}`,
  );
  // Explicit layout inset only (phone layouts pass 24); desktop defaults
  // to 0 and relies on the structural 22px column padding below — half
  // the 44px R circle (22px) overhangs the plot at 100% width, and the
  // column reserves that breathing room instead of shifting the handle.
  // Handle centres stay unclamped on the curve (see dynModel); touch
  // target + drag math untouched.
  const plotMR = $derived(props.plotMR ?? 0);
  // Bottom-left label crowding: the comp G handle rides the curve start, so
  // with a low threshold it parks at (0%, 100%) on top of both "-60" axis
  // ticks; the gate operating dot parks there too whenever silent/frozen.
  // Either occupant gets the corner tick labels relocated (never hidden).
  const compG = $derived(
    kind === 'comp' && params ? dynHandles(kind, params).find((h) => h.id === 'makeup') : null,
  );
  const gCrowdsCorner = $derived(!!compG && compG.left < 12 && compG.top > 78);
  const dotParked = $derived(
    kind === 'gate' &&
      frozen &&
      (parseFloat(props.staticDotL ?? '50') || 0) < 12 &&
      (parseFloat(props.staticDotT ?? '0') || 0) > 78,
  );
  const crowdFirst = $derived(gCrowdsCorner || dotParked);

  let dot: HTMLSpanElement | null = $state(null);

  function paint(): void {
    const level = frozen || !getLevel || !params ? -90 : getLevel();
    if (!params) return;
    const d = dynDotNums(kind, params, level);
    if (dot) {
      dot.style.left = d.left.toFixed(1) + '%';
      dot.style.top = d.top.toFixed(1) + '%';
    }
  }

  $effect(() => {
    paint();
    if (frozen || !getLevel) return;
    let raf = 0;
    const loop = (): void => {
      paint();
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(raf);
  });

  // Graph drag: horizontal moves the threshold (both kinds); on comp,
  // vertical moves the ratio with follow-the-line mapping (the curve end
  // tracks the finger; below knee height it pins at 20:1). Emits RAW spec
  // values through `thr` / `ratio` for the layout `on` action
  // (channel.dynKnob, lock-checked there). Direction lock with slop:
  // gate ignores vertical (touch-action pan-y lets the page scroll
  // natively on touch); comp needs both axes, so it takes touch
  // (touch-action none) and the lock decides threshold vs ratio per
  // gesture. Handle drags below reuse the same raw-value mapping.
  let plot: HTMLDivElement | null = $state(null);
  let drag: {
    x0: number;
    y0: number;
    px: number;
    py: number;
    w: number;
    h: number;
    mode: 'thr' | 'ratio' | null;
    el: HTMLElement | null;
  } | null = null;

  function down(e: PointerEvent): void {
    if (!onEvent || e.button !== 0) return;
    const r = plot?.getBoundingClientRect();
    if (!r || r.width < 1 || r.height < 1) return;
    drag = {
      x0: e.clientX,
      y0: e.clientY,
      px: r.left,
      py: r.top,
      w: r.width,
      h: r.height,
      mode: null,
      el: e.currentTarget as HTMLElement,
    };
  }
  function move(e: PointerEvent): void {
    if (!drag || !onEvent) return;
    const ax = Math.abs(e.clientX - drag.x0);
    const ay = Math.abs(e.clientY - drag.y0);
    if (kind === 'gate') {
      // Threshold only: vertical falls through (touch-action pan-y lets
      // the page scroll natively on touch).
      if (ax <= 10) return;
      if (!drag.mode) {
        try {
          drag.el?.setPointerCapture(e.pointerId);
        } catch {
          /* noop */
        }
        drag.mode = 'thr';
      }
    } else if (!drag.mode) {
      if (ax <= 10 && ay <= 10) return;
      try {
        drag.el?.setPointerCapture(e.pointerId);
      } catch {
        /* noop */
      }
      drag.mode = ax >= ay ? 'thr' : 'ratio';
    }
    if (drag.mode === 'thr') {
      onEvent('thr', { value: dynThrRaw(kind, (e.clientX - drag.px) / drag.w) });
    } else if (drag.mode === 'ratio') {
      onEvent('ratio', {
        value: dynRatioRaw((e.clientY - drag.py) / drag.h, compThrMakeup.thr, compThrMakeup.makeup),
      });
    }
  }
  function up(): void {
    if (drag?.mode) tap();
    drag = null;
  }

  // Handle drags (G/T/R circles): capture immediately on press like the
  // EQ nodes (no direction lock — the pending phase starved the tiny
  // buttons of moves before capture). Each handle maps one axis only
  // (T: horizontal threshold; R: vertical ratio; G: vertical makeup), so
  // off-axis movement is naturally ignored. Touch scrolling is preserved
  // by touch-action (T: pan-y, R/G: pan-x; the comp plot takes none) via
  // pointercancel, same as the EQ graph.
  const handles = $derived(params ? dynHandles(kind, params) : []);
  let hdrag: {
    id: DynHandle['id'];
    px: number;
    py: number;
    w: number;
    h: number;
    moved: boolean;
  } | null = null;

  function hdown(id: DynHandle['id'], e: PointerEvent): void {
    if (!onEvent) return;
    if (e.pointerType === 'mouse' && e.button !== 0) return;
    e.stopPropagation();
    const r = plot?.getBoundingClientRect();
    if (!r || r.width < 1 || r.height < 1) return;
    try {
      (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    } catch {
      /* noop */
    }
    hdrag = { id, px: r.left, py: r.top, w: r.width, h: r.height, moved: false };
  }
  function hmove(e: PointerEvent): void {
    if (!hdrag || !onEvent) return;
    e.stopPropagation();
    hdrag.moved = true;
    if (hdrag.id === 'thr') {
      onEvent('thr', { value: dynThrRaw(kind, (e.clientX - hdrag.px) / hdrag.w) });
    } else if (hdrag.id === 'ratio') {
      onEvent('ratio', {
        value: dynRatioRaw((e.clientY - hdrag.py) / hdrag.h, compThrMakeup.thr, compThrMakeup.makeup),
      });
    } else {
      onEvent('makeup', { value: dynMakeupRaw((e.clientY - hdrag.py) / hdrag.h) });
    }
  }
  function hup(e: PointerEvent): void {
    if (!hdrag) return;
    e.stopPropagation();
    if (hdrag.moved) tap();
    hdrag = null;
  }
  function hkey(h: DynHandle, e: KeyboardEvent): void {
    if (!onEvent) return;
    const s = spec(dynSpecKey(kind, h.id));
    const linStep = s.step && !s.log ? s.step / (s.max - s.min) : 0.02;
    const cur = toN(s, h.value);
    let n: number | null = null;
    if (e.key === 'ArrowUp' || e.key === 'ArrowRight') n = cur + linStep;
    else if (e.key === 'ArrowDown' || e.key === 'ArrowLeft') n = cur - linStep;
    else if (e.key === 'PageUp') n = cur + Math.max(linStep * 10, 0.1);
    else if (e.key === 'PageDown') n = cur - Math.max(linStep * 10, 0.1);
    else if (e.key === 'Home') n = 0;
    else if (e.key === 'End') n = 1;
    else return;
    e.preventDefault();
    e.stopPropagation();
    onEvent(h.id, { value: fromN(s, clamp(n, 0, 1)) });
  }
  function htext(h: DynHandle): string {
    return spec(dynSpecKey(kind, h.id)).fmt(h.value);
  }
</script>

<style>
  button.dyn-handle:focus-visible {
    outline: 2px solid #7cc4ff;
    outline-offset: 1px;
  }
</style>

<div
  data-signal-plot
  style="box-sizing:content-box;flex:{flexCss}{widthCss};min-height:{minH}px;display:flex;gap:10px;overflow:visible;padding:16px 16px 12px 6px;border-radius:3px;border:1px solid var(--u4-line-panel);background:var(--u4-well-bg)"
>
  <div aria-hidden="true" style="box-sizing:content-box;width:34px;flex:none;position:relative;margin-bottom:24px">
    {#each axis as a, i (a.label)}
      <span
        style="position:absolute;right:0;top:{a.top}%;transform:translateY(-50%);font-size:12px;color:var(--u4-text-8);font-variant-numeric:tabular-nums{crowdFirst &&
        i === 0
          ? ';margin-top:-30px'
          : ''}"
        >{a.label}</span
      >
    {/each}
  </div>
  <div style="box-sizing:content-box;flex:1;min-width:0;display:flex;flex-direction:column;gap:6px;overflow:visible{plotMR
    ? ''
    : ';padding-right:22px'}">
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      bind:this={plot}
      onpointerdown={down}
      onpointermove={move}
      onpointerup={up}
      onpointercancel={up}
      title={kind === 'gate' ? 'Drag horizontally: threshold' : 'Drag: threshold (horizontal), ratio (vertical)'}
      style="box-sizing:content-box;flex:1;position:relative;overflow:visible;border-radius:3px;border:1px solid var(--u4-line-panel);background:var(--u4-well-bg-2);touch-action:{kind ===
      'gate'
        ? 'pan-y'
        : 'none'};cursor:{kind === 'gate' ? 'ew-resize' : 'move'}{plotMR ? `;margin-right:${plotMR}px` : ''}"
    >
      <span aria-hidden="true" style="position:absolute;right:10px;bottom:8px;font-size:12px;color:var(--u4-text-10)">Input →</span>
      <span aria-hidden="true" style="position:absolute;left:10px;top:8px;font-size:12px;color:var(--u4-text-10)">↑ Output</span>
      <svg
        viewBox="0 0 200 200"
        preserveAspectRatio="none"
        style="position:absolute;inset:0;width:100%;height:100%;display:block"
        role="img"
        aria-label={props.aria ?? ''}
      >
        {#each grid as g (g.p)}
          <line
            x1={g.p}
            y1="0"
            x2={g.p}
            y2="200"
            stroke="var(--u4-line-row)"
            stroke-width="1"
            vector-effect="non-scaling-stroke"
          ></line>
          <line
            x1="0"
            y1={g.q}
            x2={200}
            y2={g.q}
            stroke="var(--u4-line-row)"
            stroke-width="1"
            vector-effect="non-scaling-stroke"
          ></line>
        {/each}
        <line
          x1="0"
          y1="200"
          x2="200"
          y2="0"
          stroke="var(--u4-graph-unity)"
          stroke-width="1"
          stroke-dasharray="5 5"
          vector-effect="non-scaling-stroke"
        ></line>
        <path d={props.fill ?? ''} fill={props.fillColor ?? 'transparent'} stroke="none"></path>
        <path
          d={props.path ?? ''}
          fill="none"
          stroke="var(--u4-detail-curve, #f0d36a)"
          stroke-width="3"
          stroke-linejoin="round"
          vector-effect="non-scaling-stroke"
        ></path>
      </svg>
      <span
        style="position:absolute;top:8px;left:{props.thrLeft ??
          '50'}%;transform:translateX(-50%);padding:3px 7px;border-radius:3px;background:{color};color:#0b0f14;font-size:12px;font-weight:600;white-space:nowrap;font-variant-numeric:tabular-nums"
        >{props.thrText ?? ''}</span
      >
      <span
        bind:this={dot}
        style="position:absolute;left:{props.staticDotL ??
          '0'}%;top:{props.staticDotT ??
          '100'}%;width:14px;height:14px;margin:-7px 0 0 -7px;border-radius:50%;background:#ffffff;box-shadow:0 0 0 4px rgba(255,255,255,.16)"
      ></span>
      {#each handles as h (h.id)}
        <!-- svelte-ignore a11y_role_has_required_aria_props -->
        <button
          class="dyn-handle"
          role="slider"
          aria-label={h.aria}
          aria-valuetext={htext(h)}
          onpointerdown={(e) => hdown(h.id, e)}
          onpointermove={hmove}
          onpointerup={hup}
          onpointercancel={hup}
          onkeydown={(e) => hkey(h, e)}
          title={h.aria + ' ' + htext(h)}
          style="position:absolute;left:{h.left}%;top:{h.top}%;width:44px;height:44px;transform:translate(-50%, -50%);display:flex;align-items:center;justify-content:center;padding:0;border:0;background:transparent;touch-action:{h.id ===
          'thr'
            ? 'pan-y'
            : 'pan-x'};cursor:grab"
          ><span
            style="width:36px;height:36px;border-radius:50%;border:2px solid {color};background:#f2f5f9;color:#0b0f14;font-size:15px;font-weight:700;display:flex;align-items:center;justify-content:center"
            >{h.label}</span
          ></button
        >
      {/each}
    </div>
    <div aria-hidden="true" style="box-sizing:content-box;height:18px;flex:none;position:relative">
      {#each axis as a, i (a.label)}
        <span
          style="position:absolute;top:0;left:{a.left}%;transform:translateX({a.shift});font-size:12px;color:var(--u4-text-8);font-variant-numeric:tabular-nums{crowdFirst &&
          i === 0
            ? ';margin-left:36px'
            : ''}"
          >{a.label}</span
        >
      {/each}
    </div>
  </div>
  {#if params}
    <GrBar {kind} {color} {params} {frozen} {getLevel} />
  {/if}
</div>

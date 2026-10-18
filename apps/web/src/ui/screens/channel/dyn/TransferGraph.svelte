<!-- W5b `dyn-transfer-graph` module: the gate/compressor transfer-curve
     card. Presentational: precomputed geometry (via layout `bind` from the
     dynModel port of dynVals) + live params in, nothing out (the design
     implements no graph drag; threshold "interaction" is the Threshold
     knob). Verbatim from design-template.html (isDynTab graph card):
     y-axis gutter, plot box (grid, unity diagonal, fill, threshold line,
     gold curve, threshold tag, operating dot, Input/Output captions),
     x-axis strip, GR column.
     The curve/threshold paint reactively on param edits; the operating dot
     paints imperatively via rAF from getLevel (SPEC 2a); frozen fixture
     mode paints the silent dot once from staticDotL/staticDotT. -->
<script lang="ts">
  import { dynDotNums, type DynAxisTick, type DynGridLine, type DynKind, type DynParams } from './dynModel.ts';
  import type { ModuleContext } from '../../../modules/types.ts';
  import GrBar from './GrBar.svelte';

  let {
    props = {},
    ctx: _ctx,
    nodeId: _nodeId,
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
    };
    ctx: ModuleContext;
    nodeId: string;
  } = $props();

  const kind = $derived(props.kind ?? 'comp');
  const color = $derived(props.color ?? '#ff9f43');
  const grid = $derived(props.grid ?? []);
  const axis = $derived(props.axis ?? []);
  const params = $derived(props.params ?? null);
  const frozen = $derived(props.frozen ?? true);
  const getLevel = $derived(props.getLevel ?? null);

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
</script>

<div
  style="box-sizing:content-box;flex:1 1 400px;min-height:300px;display:flex;gap:10px;padding:12px 12px 12px 6px;border-radius:3px;border:1px solid #1f2836;background:#0a0f17"
>
  <div aria-hidden="true" style="box-sizing:content-box;width:34px;flex:none;position:relative;margin-bottom:24px">
    {#each axis as a (a.label)}
      <span
        style="position:absolute;right:0;top:{a.top}%;transform:translateY(-50%);font-size:12px;color:#7d889b;font-variant-numeric:tabular-nums"
        >{a.label}</span
      >
    {/each}
  </div>
  <div style="box-sizing:content-box;flex:1;min-width:0;display:flex;flex-direction:column;gap:6px">
    <div
      style="box-sizing:content-box;flex:1;position:relative;overflow:hidden;border-radius:3px;border:1px solid #1f2836;background:#0b111b"
    >
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
            stroke="#18212f"
            stroke-width="1"
            vector-effect="non-scaling-stroke"
          ></line>
          <line
            x1="0"
            y1={g.q}
            x2={200}
            y2={g.q}
            stroke="#18212f"
            stroke-width="1"
            vector-effect="non-scaling-stroke"
          ></line>
        {/each}
        <line
          x1="0"
          y1="200"
          x2="200"
          y2="0"
          stroke="#34445e"
          stroke-width="1"
          stroke-dasharray="5 5"
          vector-effect="non-scaling-stroke"
        ></line>
        <path d={props.fill ?? ''} fill={props.fillColor ?? 'transparent'} stroke="none"></path>
        <line
          x1={props.thrX ?? '0'}
          y1="0"
          x2={props.thrX ?? '0'}
          y2="200"
          stroke={color}
          stroke-width="1.5"
          stroke-dasharray="4 4"
          vector-effect="non-scaling-stroke"
        ></line>
        <path
          d={props.path ?? ''}
          fill="none"
          stroke="#f0d36a"
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
      <span style="position:absolute;left:10px;bottom:8px;font-size:12px;color:#6c788c">Input →</span>
      <span style="position:absolute;left:10px;top:8px;font-size:12px;color:#6c788c">↑ Output</span>
    </div>
    <div aria-hidden="true" style="box-sizing:content-box;height:18px;flex:none;position:relative">
      {#each axis as a (a.label)}
        <span
          style="position:absolute;top:0;left:{a.left}%;transform:translateX({a.shift});font-size:12px;color:#7d889b;font-variant-numeric:tabular-nums"
          >{a.label}</span
        >
      {/each}
    </div>
  </div>
  {#if params}
    <GrBar {kind} {color} {params} {frozen} {getLevel} />
  {/if}
</div>

<!-- C3 GR meter column (the right side of the transfer-graph card).
     Presentational: kind + params + a live level reader in, nothing out.
     Verbatim from design-template.html (the 48px GR column: label, 18px
     track, value text). Paints imperatively via rAF (SPEC 2a: no reactive
     hot path) like the P1 Meter; frozen fixture mode paints silence once. -->
<script lang="ts">
  import { dynGrNums, type DynKind, type DynParams } from './dynModel.ts';

  let {
    kind,
    color,
    params,
    frozen,
    getLevel,
  }: {
    kind: DynKind;
    color: string;
    params: DynParams;
    frozen: boolean;
    /** Live level reader (dBFS). Ignored when frozen. */
    getLevel: (() => number) | null;
  } = $props();

  let fill: HTMLSpanElement | null = $state(null);
  let text: HTMLSpanElement | null = $state(null);

  function paint(): void {
    const level = frozen || !getLevel ? -90 : getLevel();
    const g = dynGrNums(kind, params, level);
    if (fill) fill.style.height = g.pct.toFixed(1) + '%';
    if (text) text.textContent = g.text;
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
  style="box-sizing:content-box;width:48px;flex:none;display:flex;flex-direction:column;align-items:center;gap:6px;margin-bottom:24px"
>
  <span style="font-size:12px;color:#8f9bb0">GR</span>
  <div
    style="box-sizing:content-box;flex:1;width:18px;position:relative;overflow:hidden;border-radius:3px;background:#1a2332"
  >
    <span
      bind:this={fill}
      style="position:absolute;left:0;right:0;top:0;height:0%;background:{color}"
    ></span>
  </div>
  <span bind:this={text} style="font-size:13px;font-variant-numeric:tabular-nums">0.0</span>
</div>

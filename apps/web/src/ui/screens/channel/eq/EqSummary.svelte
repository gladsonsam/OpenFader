<!-- W5b `eq-summary` module: the right-aligned selected-band summary line
     (`Low · 120 Hz · +0.5 dB · Q 0.70`). Presentational: text in, no
     events out. -->
<script lang="ts">
  import type { ModuleContext } from '../../../modules/types.ts';

  let {
    props = {},
    ctx: _ctx,
    nodeId: _nodeId,
  }: {
    props: { text?: string };
    ctx: ModuleContext;
    nodeId: string;
  } = $props();

  // Wrap only between "·" segments: each "value unit" chunk stays on one
  // line so a unit (dB, Hz, Q) can never strand onto its own line.
  const parts = $derived((props.text ?? '').split(' · '));
</script>

<div style="box-sizing:content-box;flex:1;min-width:160px;text-align:right;font-size:13px;color:var(--u4-text-4)">
  {#each parts as p, i}
    {#if i > 0}<span aria-hidden="true">{' · '}</span>{/if}<span style="white-space:nowrap">{p}</span>
  {/each}
</div>

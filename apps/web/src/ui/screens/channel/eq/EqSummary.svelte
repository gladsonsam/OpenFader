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
</script>

<div style="box-sizing:content-box;flex:1;min-width:160px;text-align:right;font-size:13px;color:var(--u4-text-4)">
  {props.text ?? ''}
</div>

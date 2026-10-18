<!-- W5b `channel-source-label`: master strip source line (bottom of the
     strip, 18px, e.g. 'Lcl 02'). No events. -->
<script lang="ts">
  import type { ModuleContext } from '../../modules/types.ts';

  let {
    props = {},
    ctx: _ctx,
    nodeId: _nodeId,
  }: {
    props: { src?: string };
    ctx: ModuleContext;
    nodeId: string;
  } = $props();
</script>

<div
  style="box-sizing:content-box;height:18px;flex:none;text-align:center;font-size:13px;color:#a7b1c0"
>
  {props.src ?? ''}
</div>

<!-- L1 module `status-dot`: connection status dot (colour + halo bound).
     Presentational: props in, no events. Style verbatim from the design. -->
<script lang="ts">
  import type { ModuleContext } from '../../modules/types.ts';

  let {
    props = {},
    ctx: _ctx,
    nodeId: _nodeId,
  }: {
    props: { color?: string; halo?: string };
    ctx: ModuleContext;
    nodeId: string;
  } = $props();
</script>

<span
  style="box-sizing:content-box;width:14px;height:14px;flex:none;border-radius:50%;background:{props.color};box-shadow:0 0 0 4px {props.halo}"
></span>

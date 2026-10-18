<!-- W5b `channel-readout`: standalone dB readout line (same 18px / 13px
     centred line the aside Fader paints above its rail), so users rebuilding
     the master strip can place the readout apart from the fader. The default
     layout leaves the readout inside `channel-fader`. No events. -->
<script lang="ts">
  import type { ModuleContext } from '../../modules/types.ts';

  let {
    props = {},
    ctx: _ctx,
    nodeId: _nodeId,
  }: {
    props: { text?: string; color?: string };
    ctx: ModuleContext;
    nodeId: string;
  } = $props();
</script>

<div
  style="height:18px;flex:none;text-align:center;font-size:13px;color:{props.color ?? '#c9d1dc'}"
>
  {props.text ?? ''}
</div>

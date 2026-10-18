<!-- W5b `channel-pan-bar`: master strip pan control (aside variant, 34px +
     readout text). Thin wrapper over the P1 PanBar: props in, `input`
     (with the value) out. Null value renders the design's empty well. -->
<script lang="ts">
  import PanBar from '../../primitives/signal/PanBar.svelte';
  import type { ModuleContext } from '../../modules/types.ts';

  let {
    props = {},
    ctx: _ctx,
    nodeId: _nodeId,
    onEvent,
  }: {
    props: { value?: number | null; name?: string; fine?: boolean };
    ctx: ModuleContext;
    nodeId: string;
    onEvent?: (name: string, payload?: unknown) => void;
  } = $props();
</script>

<PanBar
  value={props.value ?? null}
  name={props.name ?? ''}
  variant="aside"
  fine={props.fine ?? false}
  oninput={(v) => onEvent?.('input', { value: v })}
/>

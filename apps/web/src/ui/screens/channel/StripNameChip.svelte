<!-- W5b `channel-name-chip`: master strip name plate (top of the strip).
     Thin wrapper over the P1 NameBar (plate variant, 54px): props in,
     no events out. -->
<script lang="ts">
  import NameBar from '../../primitives/signal/NameBar.svelte';
  import type { ModuleContext } from '../../modules/types.ts';

  let {
    props = {},
    ctx: _ctx,
    nodeId: _nodeId,
  }: {
    props: { name?: string; color?: string; text?: string };
    ctx: ModuleContext;
    nodeId: string;
  } = $props();
</script>

<NameBar
  name={props.name ?? ''}
  sub=""
  color={props.color ?? 'var(--u4-btn-bg)'}
  text={props.text ?? 'var(--u4-text-2)'}
  variant="plate"
/>

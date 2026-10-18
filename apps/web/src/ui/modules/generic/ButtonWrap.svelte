<!-- L0 generic button module: presentational wrapper around ui Button.
     Callbacks are out of scope for layout JSON (props in only); interactive
     wiring happens in wave 3 via ModuleContext. -->
<script lang="ts">
  import Button from '../../primitives/Button.svelte';
  import type { ModuleContext } from '../types.ts';

  let {
    props = {},
    ctx: _ctx,
    nodeId,
  }: {
    props: { label?: string; variant?: 'default' | 'primary' | 'danger' | 'ghost'; height?: number };
    ctx: ModuleContext;
    nodeId: string;
  } = $props();
</script>

<Button
  label={props.label ?? nodeId}
  variant={props.variant ?? 'default'}
  height={props.height ?? 40}
  ariaLabel={props.label ?? nodeId}
/>

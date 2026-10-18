<!-- L0 generic panel module: presentational wrapper around ui Panel. -->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import Panel from '../../primitives/Panel.svelte';
  import type { ModuleContext } from '../types.ts';

  let {
    props = {},
    ctx: _ctx,
    nodeId: _nodeId,
    children,
  }: {
    props: { title?: string };
    ctx: ModuleContext;
    nodeId: string;
    children?: Snippet;
  } = $props();
</script>

<Panel title={props.title ?? 'Panel'}>
  {#if children}{@render children()}{/if}
</Panel>

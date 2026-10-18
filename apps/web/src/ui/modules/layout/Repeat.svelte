<!-- L0 repeat: transparent template expander. The renderer expands repeat nodes
     into sibling instances, so this component only exists for manifest
     validity / direct use: it renders children with NO wrapper element. -->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { ModuleContext } from '../types.ts';

  let {
    props: _props = {},
    ctx: _ctx,
    nodeId: _nodeId,
    children,
  }: {
    props: Record<string, unknown>;
    ctx: ModuleContext;
    nodeId: string;
    children?: Snippet;
  } = $props();
</script>

{@render children?.()}

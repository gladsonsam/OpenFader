<!-- L0 tabs container: ONE root element. The renderer passes the tab pages as
     `nodes` plus a `body` snippet; only the active page renders (inactive pages
     render nothing, so no hidden boxes exist). Local active state only. -->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { ModuleContext } from '../types.ts';
  import type { LayoutNode } from '../schema.ts';
  import { styleJoin } from './css.ts';
  import { tap } from '../../primitives/feel.ts';

  let {
    props = {},
    nodes = [],
    body,
    ctx: _ctx,
    nodeId,
  }: {
    props: { tabs?: string[]; active?: number };
    nodes: LayoutNode[];
    body: Snippet<[LayoutNode]>;
    ctx: ModuleContext;
    nodeId: string;
  } = $props();

  let active = $state(props.active ?? 0);
  const labels = $derived(props.tabs ?? nodes.map((n) => n.id));
  const current = $derived(nodes[Math.min(active, Math.max(0, nodes.length - 1))]);
</script>

<div data-l0="tabs" data-node={nodeId} style={styleJoin(['display:flex', 'flex-direction:column'])}>
  <div role="tablist" style="display:flex;flex-direction:row">
    {#each labels as label, i (i)}
      <button
        role="tab"
        aria-selected={i === active ? 'true' : 'false'}
        onclick={() => {
          active = i;
          tap();
        }}
        style="cursor:pointer"
      >
        {label}
      </button>
    {/each}
  </div>
  <div role="tabpanel" style="display:flex;flex-direction:column">
    {#if current}
      {@render body(current)}
    {/if}
  </div>
</div>

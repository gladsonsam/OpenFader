<!-- L0 stack container: ONE block element, normal vertical flow, no layout
     styles of its own beyond sizes/padding (children keep their own boxes). -->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { ModuleContext } from '../types.ts';
  import { sizeCss, styleJoin } from './css.ts';

  let {
    props = {},
    ctx: _ctx,
    nodeId,
    children,
  }: {
    props: { width?: number | string; height?: number | string; padding?: number | string };
    ctx: ModuleContext;
    nodeId: string;
    children?: Snippet;
  } = $props();

  const style = $derived(
    styleJoin([
      'display:block',
      sizeCss(props.width) ? `width:${sizeCss(props.width)}` : undefined,
      sizeCss(props.height) ? `height:${sizeCss(props.height)}` : undefined,
      sizeCss(props.padding) ? `padding:${sizeCss(props.padding)}` : undefined,
    ]),
  );
</script>

<div data-node={nodeId} data-l0="stack" style={style}>{@render children?.()}</div>

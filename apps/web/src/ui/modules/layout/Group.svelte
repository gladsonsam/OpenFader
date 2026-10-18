<!-- L0 group: ONE neutral block element (sizes + padding only, no flex/grid
     of its own) for grouping nodes without changing layout semantics.
     L1: tag/label/border/background/radius/margin/box for card sections. -->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { ModuleContext } from '../types.ts';
  import { boxStyle, sizeCss, styleJoin, type BoxProps } from './css.ts';

  let {
    props = {},
    ctx: _ctx,
    nodeId,
    children,
  }: {
    props: {
      width?: number | string;
      height?: number | string;
      padding?: number | string;
    } & BoxProps;
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
      ...boxStyle(props),
    ]),
  );
</script>

<svelte:element
  this={props.tag ?? 'div'}
  data-node={nodeId}
  data-l0="group"
  aria-label={props.label}
  {style}>{@render children?.()}</svelte:element
>

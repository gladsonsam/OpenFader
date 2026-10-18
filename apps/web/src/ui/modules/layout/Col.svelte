<!-- L0 col container: ONE flex-column element, exact CSS from props. No wrappers.
     L1: tag/label/border/background/radius/min-max-width/margin/box (see Row). -->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { ModuleContext } from '../types.ts';
  import { boxStyle, gapCss, sizeCss, styleJoin, type BoxProps } from './css.ts';

  let {
    props = {},
    ctx: _ctx,
    nodeId,
    children,
  }: {
    props: {
      gap?: number | string;
      align?: string;
      justify?: string;
      wrap?: boolean;
      width?: number | string;
      height?: number | string;
      flex?: number | string;
      padding?: number | string;
    } & BoxProps;
    ctx: ModuleContext;
    nodeId: string;
    children?: Snippet;
  } = $props();

  const style = $derived(
    styleJoin([
      'display:flex',
      'flex-direction:column',
      gapCss(props.gap),
      props.align ? `align-items:${props.align}` : undefined,
      props.justify ? `justify-content:${props.justify}` : undefined,
      props.wrap ? 'flex-wrap:wrap' : undefined,
      sizeCss(props.width) ? `width:${sizeCss(props.width)}` : undefined,
      sizeCss(props.height) ? `height:${sizeCss(props.height)}` : undefined,
      props.flex !== undefined ? `flex:${props.flex}` : undefined,
      sizeCss(props.padding) ? `padding:${sizeCss(props.padding)}` : undefined,
      ...boxStyle(props),
    ]),
  );
</script>

<svelte:element
  this={props.tag ?? 'div'}
  data-node={nodeId}
  data-l0="col"
  aria-label={props.label}
  {style}>{@render children?.()}</svelte:element
>

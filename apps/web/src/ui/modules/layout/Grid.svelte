<!-- L0 grid container: ONE grid element. columns/rows are template strings.
     L1: tag/label/flex/border/background/radius/min-max-width/margin/box. -->
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
      columns?: string;
      rows?: string;
      gap?: number | string;
      columnGap?: number | string;
      rowGap?: number | string;
      align?: string;
      justify?: string;
      width?: number | string;
      height?: number | string;
      flex?: number | string;
      padding?: number | string;
      /** When true, grid items snap to the scrollport start (with a `snap`
          scroll parent: the phone/tablet strip pager). Absent = off. */
      snapItems?: boolean;
    } & BoxProps;
    ctx: ModuleContext;
    nodeId: string;
    children?: Snippet;
  } = $props();

  const style = $derived(
    styleJoin([
      'display:grid',
      props.columns ? `grid-template-columns:${props.columns}` : undefined,
      props.rows ? `grid-template-rows:${props.rows}` : undefined,
      gapCss(props.gap),
      props.columnGap !== undefined && props.gap === undefined
        ? gapCss(props.columnGap)?.replace('gap:', 'column-gap:')
        : undefined,
      props.rowGap !== undefined && props.gap === undefined
        ? gapCss(props.rowGap)?.replace('gap:', 'row-gap:')
        : undefined,
      props.align ? `align-items:${props.align}` : undefined,
      props.justify ? `justify-items:${props.justify}` : undefined,
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
  data-l0="grid"
  data-snap-items={props.snapItems ? 'start' : undefined}
  aria-label={props.label}
  {style}>{@render children?.()}</svelte:element
>

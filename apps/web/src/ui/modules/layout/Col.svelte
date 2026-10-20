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
    onEvent,
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
      /** When true, clicking anywhere on the column emits `select`
          (mixer strip / master: the whole channel selects, not just the
          name bar). Child controls still get their own events first;
          scroll/selection drags suppress the click natively. */
      selectable?: boolean;
    } & BoxProps;
    ctx: ModuleContext;
    nodeId: string;
    children?: Snippet;
    onEvent?: (name: string, payload?: unknown) => void;
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
      props.selectable ? 'cursor:pointer' : undefined,
      ...boxStyle(props),
    ]),
  );

  function click(): void {
    if (props.selectable) onEvent?.('select');
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<svelte:element
  this={props.tag ?? 'div'}
  data-node={nodeId}
  data-l0="col"
  aria-label={props.label}
  onclick={props.selectable ? click : undefined}
  {style}>{@render children?.()}</svelte:element
>

<!-- L0 scroll container: ONE element, flex in `direction` with overflow auto.
     W5c: `flex` + visual box props (min-width etc.) so the mixer strip row
     can own its scroll box in layout data (additive only; defaults unchanged). -->
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
      direction?: 'row' | 'col';
      gap?: number | string;
      width?: number | string;
      height?: number | string;
      flex?: number | string;
      /** Scroll-snap axis ('x' for the phone strip scroller). Absent = off. */
      snap?: 'x' | 'y';
    } & BoxProps;
    ctx: ModuleContext;
    nodeId: string;
    children?: Snippet;
  } = $props();

  const style = $derived(
    styleJoin([
      'display:flex',
      `flex-direction:${props.direction === 'row' ? 'row' : 'column'}`,
      'overflow:auto',
      props.snap ? `scroll-snap-type:${props.snap} proximity` : undefined,
      gapCss(props.gap),
      sizeCss(props.width) ? `width:${sizeCss(props.width)}` : undefined,
      sizeCss(props.height) ? `height:${sizeCss(props.height)}` : undefined,
      props.flex !== undefined ? `flex:${props.flex}` : undefined,
      ...boxStyle(props),
    ]),
  );
</script>

<div data-node={nodeId} data-l0="scroll" style={style}>{@render children?.()}</div>

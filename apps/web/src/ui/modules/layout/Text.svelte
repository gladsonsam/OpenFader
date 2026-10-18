<!-- L0 text leaf: ONE element rendering a text prop with size/colour/align.
     L1: tag + margin so labels inside JSON-owned flex rows match the design;
     `{{ }}` holes are interpolated by the renderer before props arrive, and
     the legacy repeat `{item}` replacement is kept for old default layouts. -->
<script lang="ts">
  import type { ModuleContext } from '../types.ts';
  import { sizeCss, styleJoin } from './css.ts';

  let {
    props = {},
    ctx: _ctx,
    nodeId,
  }: {
    props: {
      value?: string;
      item?: string;
      size?: number;
      color?: string;
      align?: 'left' | 'center' | 'right';
      weight?: number;
      margin?: number | string;
      tag?: 'div' | 'span' | string;
    };
    ctx: ModuleContext;
    nodeId: string;
  } = $props();

  /** Repeat templates can interpolate the instance item: "strip {item}". */
  const text = $derived((props.value ?? '').replace('{item}', String(props.item ?? '')));
</script>

<svelte:element
  this={props.tag ?? 'div'}
  data-l0="text"
  data-node={nodeId}
  style={styleJoin([
    props.size ? `font-size:${props.size}px` : undefined,
    props.color ? `color:${props.color}` : undefined,
    props.align ? `text-align:${props.align}` : undefined,
    props.weight ? `font-weight:${props.weight}` : undefined,
    sizeCss(props.margin) ? `margin:${sizeCss(props.margin)}` : undefined,
  ])}
>
  {text}
</svelte:element>

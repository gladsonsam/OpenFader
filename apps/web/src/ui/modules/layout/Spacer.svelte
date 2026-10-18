<!-- L0 spacer leaf: ONE empty element with explicit size or flex grow. -->
<script lang="ts">
  import type { ModuleContext } from '../types.ts';
  import { sizeCss, styleJoin } from './css.ts';

  let {
    props = {},
    ctx: _ctx,
    nodeId,
  }: {
    props: { w?: number | string; h?: number | string; grow?: boolean };
    ctx: ModuleContext;
    nodeId: string;
  } = $props();

  const style = $derived(
    styleJoin([
      sizeCss(props.w) ? `width:${sizeCss(props.w)}` : undefined,
      sizeCss(props.h) ? `height:${sizeCss(props.h)}` : undefined,
      props.grow ? 'flex:1 1 auto' : undefined,
    ]),
  );
</script>

<div data-l0="spacer" data-node={nodeId} style={style}></div>

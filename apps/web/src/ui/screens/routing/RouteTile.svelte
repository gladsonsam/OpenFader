<!-- L1 module `route-tile`: one routing channel tile button.
     Presentational: props in (via layout `bind`), `press` event out.
     Inline styles verbatim from the design template (routing tiles). -->
<script lang="ts">
  import type { ModuleContext } from '../../modules/types.ts';

  let {
    props = {},
    ctx: _ctx,
    nodeId: _nodeId,
    onEvent,
  }: {
    props: {
      name?: string;
      src?: string;
      srcColor?: string;
      pressed?: string;
      border?: string;
      bg?: string;
    };
    ctx: ModuleContext;
    nodeId: string;
    onEvent?: (name: string, payload?: unknown) => void;
  } = $props();
</script>

<button
  onclick={() => onEvent?.('press')}
  aria-pressed={props.pressed}
  style="box-sizing:border-box;font-variant-numeric:normal;height:60px;display:flex;flex-direction:column;justify-content:center;gap:2px;padding:0 10px;border-radius:3px;border:2px solid {props.border};background:{props.bg};color:#eef2f7;cursor:pointer;text-align:left"
>
  <span style="font-size:15px;font-weight:600">{props.name}</span>
  <span style="font-size:12px;color:{props.srcColor}">{props.src}</span>
</button>

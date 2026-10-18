<!-- L1 module `src-tile`: one routing source tile button.
     Presentational: props in (via layout `bind`), `press` event out.
     Inline styles verbatim from the design template (routing sources). -->
<script lang="ts">
  import type { ModuleContext } from '../../modules/types.ts';

  let {
    props = {},
    ctx: _ctx,
    nodeId: _nodeId,
    onEvent,
  }: {
    props: {
      label?: string;
      used?: string;
      pressed?: string;
      bg?: string;
      fg?: string;
      border?: string;
    };
    ctx: ModuleContext;
    nodeId: string;
    onEvent?: (name: string, payload?: unknown) => void;
  } = $props();
</script>

<button
  onclick={() => onEvent?.('press')}
  aria-pressed={props.pressed}
  style="box-sizing:border-box;font-variant-numeric:normal;height:52px;display:flex;flex-direction:column;align-items:center;justify-content:center;gap:1px;padding:0 4px;border-radius:3px;border:1px solid {props.border};background:{props.bg};color:{props.fg};cursor:pointer"
>
  <span style="font-size:13px;font-weight:500">{props.label}</span>
  <span style="font-size:11px;opacity:.75;white-space:nowrap">{props.used}</span>
</button>

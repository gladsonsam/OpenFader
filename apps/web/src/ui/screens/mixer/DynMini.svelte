<!-- W5c module `strip-dyn-mini`: dynamics mini pane (bottom half of the strip
     well). Verbatim copy of the Dyn half of the strip well in
     docs/design/src/design-template.html: flex-1 button with the gold
     transfer curve (dynThumbPath from gate/comp params), `Dyn` label, aria
     `Open dynamics for Ch 1`. Taps emit `open` for the layout `on` action
     (ui.openDyn). Presentational: props in, event out. -->
<script lang="ts">
  import type { ModuleContext } from '../../modules/types.ts';

  let {
    props = {},
    ctx: _ctx,
    nodeId: _nodeId,
    onEvent,
  }: {
    props: {
      /** SVG path (viewBox 0 0 100 40) of the gate+comp transfer curve. */
      path?: string;
      /** Design aria, e.g. 'Open dynamics for Ch 1'. */
      ariaLabel?: string;
    };
    ctx: ModuleContext;
    nodeId: string;
    onEvent?: (name: string, payload?: unknown) => void;
  } = $props();
</script>

<button
  onclick={() => onEvent?.('open')}
  aria-label={props.ariaLabel ?? 'Open dynamics'}
  style="flex:1;position:relative;padding:0;border:0;background:transparent;cursor:pointer"
>
  <svg
    viewBox="0 0 100 40"
    preserveAspectRatio="none"
    style="position:absolute;inset:0;width:100%;height:100%;display:block"
    aria-hidden="true"
  >
    <path
      d={props.path ?? ''}
      fill="none"
      stroke="#c8b43c"
      stroke-width="1.4"
      vector-effect="non-scaling-stroke"
    ></path>
  </svg>
  <span style="position:absolute;left:4px;top:2px;font-size:11px;color:#6c788c">Dyn</span>
</button>

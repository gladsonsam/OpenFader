<!-- W5c module `strip-eq-mini`: EQ mini pane (top half of the strip's Dyn/EQ
     well). Verbatim copy of the EQ half of the strip well in
     docs/design/src/design-template.html (lines ~93-101): flex-1 button with
     the 0 dB centre line + yellow response curve (eqThumbPath from the band
     parameters), `EQ` label, aria `Open EQ for Ch 1`. Taps emit `open` for
     the layout `on` action (ui.openEq). Presentational: props in, event out. -->
<script lang="ts">
  import type { ModuleContext } from '../../modules/types.ts';

  let {
    props = {},
    ctx: _ctx,
    nodeId: _nodeId,
    onEvent,
  }: {
    props: {
      /** SVG path (viewBox 0 0 100 40) of the combined EQ response. */
      path?: string;
      /** Curve stroke: gold when EQ is in, dim when bypassed / no EQ. */
      stroke?: string;
      /** Design aria, e.g. 'Open EQ for Ch 1'. */
      ariaLabel?: string;
    };
    ctx: ModuleContext;
    nodeId: string;
    onEvent?: (name: string, payload?: unknown) => void;
  } = $props();
</script>

<button
  onclick={() => onEvent?.('open')}
  aria-label={props.ariaLabel ?? 'Open EQ'}
  style="flex:1;position:relative;padding:0;border:0;border-bottom:1px solid #1f2836;background:transparent;cursor:pointer"
>
  <svg
    viewBox="0 0 100 40"
    preserveAspectRatio="none"
    style="position:absolute;inset:0;width:100%;height:100%;display:block"
    aria-hidden="true"
  >
    <line
      x1="0"
      y1="20"
      x2="100"
      y2="20"
      stroke="#1d2737"
      stroke-width="1"
      vector-effect="non-scaling-stroke"
    ></line>
    <path
      d={props.path ?? ''}
      fill="none"
      stroke={props.stroke ?? '#d9c24a'}
      stroke-width="1.6"
      vector-effect="non-scaling-stroke"
    ></path>
  </svg>
  <span style="position:absolute;left:4px;top:2px;font-size:11px;color:#6c788c">EQ</span>
</button>

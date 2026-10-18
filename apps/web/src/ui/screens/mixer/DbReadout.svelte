<!-- W5c module `strip-readout`: dB readout line (the `-4.3` above each fader).
     Verbatim copy of the readout div in design-template.html: 15px, centred,
     12px, channel grey (or SOF cyan on sends). Presentational: props in.
     (The default strip reads out inside the strip-fader module; this module
     exists so custom layouts can place it on their own.) -->
<script lang="ts">
  import type { ModuleContext } from '../../modules/types.ts';

  let {
    props = {},
    ctx: _ctx,
    nodeId: _nodeId,
    onEvent: _onEvent,
  }: {
    /** Formatted dB text, e.g. '-4.3' or '-∞' (fmtDb). */
    props: {
      text?: string;
      /** Channel grey, or SOF cyan. */
      color?: string;
    };
    ctx: ModuleContext;
    nodeId: string;
    onEvent?: (name: string, payload?: unknown) => void;
  } = $props();
</script>

<div style="height:15px;flex:none;text-align:center;font-size:12px;color:{props.color ?? 'var(--u4-text-4)'}">{props.text ?? ''}</div>

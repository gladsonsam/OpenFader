<!-- W5b `channel-stepper`: master strip prev/next channel buttons.
     Presentational: no props in, `prev` / `next` events out. Button markup
     verbatim from design-template.html (channel `aside` stepper: 44px,
     chevron SVGs). -->
<script lang="ts">
  import type { ModuleContext } from '../../modules/types.ts';

  let {
    props: _props = {},
    ctx: _ctx,
    nodeId: _nodeId,
    onEvent,
  }: {
    props: Record<string, never>;
    ctx: ModuleContext;
    nodeId: string;
    onEvent?: (name: string, payload?: unknown) => void;
  } = $props();
</script>

<div style="box-sizing:content-box;display:flex;gap:5px;flex:none">
  <button
    onclick={() => onEvent?.('prev')}
    aria-label="Previous channel"
    style="box-sizing:border-box;font-variant-numeric:normal;flex:1;height:44px;border-radius:3px;border:1px solid #3a414c;background:#2b3038;color:#e8edf5;cursor:pointer;display:flex;align-items:center;justify-content:center"
  >
    <svg
      width="20"
      height="20"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="2.2"
      stroke-linecap="round"
      stroke-linejoin="round"
      aria-hidden="true"><path d="M15 6l-6 6 6 6"></path></svg
    >
  </button>
  <button
    onclick={() => onEvent?.('next')}
    aria-label="Next channel"
    style="box-sizing:border-box;font-variant-numeric:normal;flex:1;height:44px;border-radius:3px;border:1px solid #3a414c;background:#2b3038;color:#e8edf5;cursor:pointer;display:flex;align-items:center;justify-content:center"
  >
    <svg
      width="20"
      height="20"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="2.2"
      stroke-linecap="round"
      stroke-linejoin="round"
      aria-hidden="true"><path d="M9 6l6 6-6 6"></path></svg
    >
  </button>
</div>

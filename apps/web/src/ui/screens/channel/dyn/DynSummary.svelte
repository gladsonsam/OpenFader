<!-- W5b `dyn-summary` module: the dyn tab top row (In toggle + summary).
     Presentational: toggle visuals + summary text in via layout `bind`,
     `toggle` out. Verbatim from design-template.html (isDynTab head row).
     The toggle keeps the design's look when locked (the store blocks the
     edit, like dvToggle). -->
<script lang="ts">
  import type { ModuleContext } from '../../../modules/types.ts';

  let {
    props = {},
    ctx: _ctx,
    nodeId: _nodeId,
    onEvent,
  }: {
    props: {
      label?: string;
      bg?: string;
      fg?: string;
      pressed?: string;
      summary?: string;
    };
    ctx: ModuleContext;
    nodeId: string;
    onEvent?: (name: string, payload?: unknown) => void;
  } = $props();

  // Wrap only between "·" segments: each "value unit" chunk stays on one
  // line so a unit (dB, ms, Hz) can never strand onto its own line.
  const parts = $derived((props.summary ?? '').split(' · '));
</script>

<div
  style="box-sizing:content-box;flex:none;display:flex;flex-wrap:wrap;align-items:center;gap:8px"
>
  <button
    onclick={() => onEvent?.('toggle')}
    aria-pressed={props.pressed ?? 'false'}
    style="box-sizing:border-box;font-variant-numeric:normal;height:40px;padding:0 16px;border-radius:3px;border:1px solid var(--u4-line-ctl);background:{props.bg ??
      'var(--u4-btn-bg)'};color:{props.fg ?? 'var(--u4-text-4)'};font-size:15px;cursor:pointer"
    ><span>{props.label ?? ''}</span></button
  >
  <div
    style="box-sizing:content-box;flex:1;min-width:160px;text-align:right;font-size:13px;color:var(--u4-text-4)"
  >
    {#each parts as p, i}
      {#if i > 0}<span aria-hidden="true">{' · '}</span>{/if}<span style="white-space:nowrap">{p}</span>
    {/each}
  </div>
</div>

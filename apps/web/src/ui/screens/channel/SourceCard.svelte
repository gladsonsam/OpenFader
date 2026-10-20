<!-- W5b `input-source-card`: the Input tab Source card (patch name/group,
     input level text + hbar meter, patch button). Presentational: the
     InputVm + formatted level text + live meter feed in via layout `bind`,
     `patch` out. Verbatim from design-template.html (isInputTab). -->
<script lang="ts">
  import Meter, { type MeterSource } from '../../primitives/signal/Meter.svelte';
  import type { ModuleContext } from '../../modules/types.ts';
  import type { InputVm } from './channelModel.ts';

  let {
    props = {},
    ctx: _ctx,
    nodeId: _nodeId,
    onEvent,
  }: {
    props: {
      vm?: InputVm | null;
      levelText?: string;
      meter?: MeterSource | null;
      meterFrozen?: boolean;
      showMeter?: boolean;
    };
    ctx: ModuleContext;
    nodeId: string;
    onEvent?: (name: string, payload?: unknown) => void;
  } = $props();

  const vm = $derived(props.vm ?? ({ hasInput: false } as InputVm));
</script>

{#if vm.hasInput}
<section
  aria-label="Source"
  style="box-sizing:content-box;display:flex;flex-direction:column;gap:10px;padding:10px;border-radius:3px;border:1px solid var(--u4-line-panel);background:var(--u4-panel-bg)"
>
  <div style="display:flex;align-items:center;gap:12px;justify-content:space-between">
    <div style="min-width:0;display:flex;flex-direction:column;gap:4px">
      <span style="font-size:12px;color:var(--u4-text-6)">Input source</span>
      <span style="font-size:16px;font-weight:600;color:var(--u4-text-1)">{vm.srcName} <span style="font-size:12px;font-weight:400;color:var(--u4-text-5)">{vm.srcGroup}</span></span>
    </div>
    <button
      onclick={() => onEvent?.('patch')}
      aria-label="Change input source in Routing"
      style="flex:none;box-sizing:border-box;min-height:44px;padding:0 12px;border-radius:3px;border:1px solid var(--u4-line-ctl);background:var(--u4-btn-bg);color:var(--u4-text-1);font-size:14px;cursor:pointer"
    >Change</button>
  </div>
  {#if props.showMeter !== false}
  <div style="box-sizing:content-box;display:flex;flex-direction:column;gap:6px">
    <div style="box-sizing:content-box;display:flex;justify-content:space-between;font-size:12px;color:var(--u4-text-6)">
      <span>Input level</span><span style="color:var(--u4-text-1)">{props.levelText ?? ''}</span>
    </div>
    <Meter
      variant="hbar"
      source={props.meter ?? { getLevel: () => -90 }}
      frozen={props.meterFrozen ?? false}
    />
  </div>
  {/if}
</section>
{/if}

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
  <div style="font-size:12px;color:var(--u4-text-6)">Source</div>
  <div style="box-sizing:content-box;display:flex;align-items:baseline;gap:10px">
    <span style="font-size:34px;font-weight:600;">{vm.srcName}</span>
    <span style="font-size:14px;color:var(--u4-text-5)">{vm.srcGroup}</span>
  </div>
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
  <button
    onclick={() => onEvent?.('patch')}
    style="box-sizing:border-box;font-variant-numeric:normal;height:48px;margin-top:auto;border-radius:3px;border:1px solid var(--u4-line-ctl);background:var(--u4-btn-bg);color:var(--u4-text-1);font-size:14px;cursor:pointer"
    >Change patch in Routing</button
  >
</section>
{/if}

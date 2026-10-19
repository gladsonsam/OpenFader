<!-- W5b `input-delay-card`: the Input tab Delay card (on/off toggle, Time
     knob, distance text). Presentational: the InputVm in via layout `bind`,
     `knob` ({key:'delay',value}) / `toggle` out. Verbatim from
     design-template.html (isInputTab). -->
<script lang="ts">
  import Knob from '../../primitives/signal/Knob.svelte';
  import type { ModuleContext } from '../../modules/types.ts';
  import type { InputVm } from './channelModel.ts';

  let {
    props = {},
    ctx: _ctx,
    nodeId: _nodeId,
    onEvent,
  }: {
    props: { vm?: InputVm | null; fine?: boolean };
    ctx: ModuleContext;
    nodeId: string;
    onEvent?: (name: string, payload?: unknown) => void;
  } = $props();

  const vm = $derived(props.vm ?? ({ hasInput: false } as InputVm));
</script>

{#if vm.hasInput}
<section
  aria-label="Delay"
  style="box-sizing:content-box;display:flex;flex-direction:column;gap:10px;padding:10px;border-radius:3px;border:1px solid var(--u4-line-panel);background:var(--u4-panel-bg)"
>
  <div style="box-sizing:content-box;display:flex;align-items:center;justify-content:space-between">
    <span style="font-size:12px;color:var(--u4-text-6)">Delay</span>
    <button
      onclick={() => onEvent?.('toggle')}
      aria-pressed={vm.delayOn ? 'true' : 'false'}
      style="box-sizing:border-box;font-variant-numeric:normal;height:34px;padding:0 14px;border-radius:3px;border:1px solid var(--u4-line-ctl);background:{vm.delayBg};color:{vm.delayFg};font-size:13px;cursor:pointer"
      >{vm.delayLabel}</button
    >
  </div>
  <div style="box-sizing:content-box;display:flex;justify-content:space-around;gap:6px">
    <Knob
      specKey="delay"
      value={vm.delayMs}
      label="Time"
      color={vm.delayKnobColor}
      size={68}
      defaultValue={0}
      fine={props.fine ?? false}
      oninput={(v) => onEvent?.('knob', { key: 'delay', value: v })}
    />
  </div>
  <div style="font-size:13px;color:var(--u4-text-5);text-align:center">{vm.delayDist}</div>
</section>
{/if}

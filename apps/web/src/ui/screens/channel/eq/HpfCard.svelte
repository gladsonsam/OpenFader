<!-- W5b `eq-hpf-card` module: the HPF card. Presentational: the
     EqHpfCardVm in via layout `bind`, `select` / `toggle` /
     `slope` ({slope}) / `freq` ({value}) events out. Verbatim from
     design-template.html (12/24 dB/oct segmented, single centred 58px P1
     Knob in `eq` mode for band 0, whose norm is log(f/20)/log(50) per the
     design's HPF knob).
     requires/fallback: desks without `hpf_slope` set showSlope=false and the
     slope segmented row is hidden (collapsed), never faked. -->
<script lang="ts">
  import Knob from '../../../primitives/signal/Knob.svelte';
  import type { ModuleContext } from '../../../modules/types.ts';
  import type { EqHpfCardVm } from './eqModel.ts';

  let {
    props = {},
    ctx: _ctx,
    nodeId: _nodeId,
    onEvent,
  }: {
    props: {
      hpf?: EqHpfCardVm | null;
      fine?: boolean;
      locked?: boolean;
      /** False on desks without hpf_slope: the slope row collapses. */
      showSlope?: boolean;
    };
    ctx: ModuleContext;
    nodeId: string;
    onEvent?: (name: string, payload?: unknown) => void;
  } = $props();

  const vm = $derived(props.hpf);
  const showSlope = $derived(props.showSlope ?? true);
</script>

{#if vm}
  <div
    style="box-sizing:content-box;display:flex;flex-direction:column;gap:8px;padding:8px;border-radius:6px;border:2px solid {vm.border};background:var(--u4-panel-bg)"
  >
    <div style="box-sizing:content-box;display:flex;align-items:center;gap:6px">
      <button
        onclick={() => onEvent?.('select', { band: 0 })}
        aria-label="Select HPF band"
        style="box-sizing:border-box;font-variant-numeric:normal;flex:1;min-width:0;height:32px;display:flex;align-items:center;gap:8px;padding:0;border:0;background:transparent;color:var(--u4-text-2);font-size:15px;font-weight:600;text-align:left;cursor:pointer"
      >
        <span style="width:12px;height:12px;flex:none;border-radius:50%;background:{vm.color}"></span>
        HPF
      </button>
      <button
        onclick={() => onEvent?.('toggle', { band: 0 })}
        aria-pressed={vm.onStr}
        aria-label={vm.toggleAria}
        style="box-sizing:border-box;font-variant-numeric:normal;height:32px;padding:0 10px;border-radius:4px;border:1px solid var(--u4-line-ctl);background:{vm.onBg};color:{vm.onText};font-size:13px;cursor:pointer"
        >{vm.onLabel}</button
      >
    </div>
    {#if showSlope}
      <div style="box-sizing:content-box;display:flex;gap:4px">
        {#each [12, 24] as s (s)}
          {@const active = vm.slope === s}
          <button
            onclick={() => onEvent?.('slope', { slope: s })}
            aria-pressed={active ? 'true' : 'false'}
            aria-label={'HPF slope ' + s + ' dB per octave'}
            style="box-sizing:border-box;font-variant-numeric:normal;flex:1;height:30px;border-radius:4px;border:1px solid var(--u4-line-ctl);background:{active
              ? 'var(--u4-choice-on-bg, var(--u4-text-1))'
              : 'var(--u4-btn-bg)'};color:{active ? 'var(--u4-seg-on-fg)' : 'var(--u4-text-4)'};font-size:13px;cursor:pointer"
            >{s} dB/oct</button
          >
        {/each}
      </div>
    {/if}
    <div style="box-sizing:content-box;display:flex;justify-content:space-around;gap:2px">
      <Knob
        specKey="gain"
        mode="eq"
        eqBand={0}
        eqParam="f"
        value={vm.freq}
        label="Freq"
        color={vm.color}
        size={58}
        defaultValue={80}
        fine={props.fine ?? false}
        oninput={(v) => {
          if (!props.locked) onEvent?.('freq', { value: v });
        }}
      />
    </div>
  </div>
{/if}

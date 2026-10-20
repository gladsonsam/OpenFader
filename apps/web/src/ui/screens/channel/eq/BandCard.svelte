<!-- W5b `eq-band-card` module: one parametric band card (Low / Lo-Mid /
     Hi-Mid / High). Presentational: the EqBandCardVm in via layout `bind`,
     `select` / `toggle` / `type` ({type}) / `param` ({param,value}) events
     out. Card chrome verbatim from design-template.html (2px
     selected-colour border, 12px colour dot, On/Off toggle, Shelf/Bell
     segmented, three 58px P1 Knobs in `eq` mode with the band accent
     colour). -->
<script lang="ts">
  import Knob from '../../../primitives/signal/Knob.svelte';
  import type { ModuleContext } from '../../../modules/types.ts';
  import type { EqBandCardVm } from './eqModel.ts';

  let {
    props = {},
    ctx: _ctx,
    nodeId: _nodeId,
    onEvent,
  }: {
    props: {
      card?: EqBandCardVm | null;
      fine?: boolean;
      locked?: boolean;
      showType?: boolean;
    };
    ctx: ModuleContext;
    nodeId: string;
    onEvent?: (name: string, payload?: unknown) => void;
  } = $props();

  const vm = $derived(props.card);
  const showType = $derived(props.showType ?? true);

  function param(param: 'f' | 'g' | 'q', v: number): void {
    if (props.locked || !vm) return;
    onEvent?.('param', { band: vm.band, param, value: v });
  }
</script>

{#if vm}
  <div
    style="box-sizing:content-box;display:flex;flex-direction:column;gap:8px;padding:8px;border-radius:6px;border:2px solid {vm.border};background:var(--u4-panel-bg)"
  >
    <div style="box-sizing:content-box;display:flex;align-items:center;gap:6px">
      <button
        onclick={() => onEvent?.('select', { band: vm.band })}
        aria-label={'Select ' + vm.name + ' band'}
        style="box-sizing:border-box;font-variant-numeric:normal;flex:1;min-width:0;height:32px;display:flex;align-items:center;gap:8px;padding:0;border:0;background:transparent;color:var(--u4-text-2);font-size:15px;font-weight:600;text-align:left;cursor:pointer"
      >
        <span style="width:12px;height:12px;flex:none;border-radius:50%;background:{vm.color}"></span>
        {vm.name}
      </button>
      <button
        onclick={() => onEvent?.('toggle', { band: vm.band })}
        aria-pressed={vm.onStr}
        aria-label={vm.toggleAria}
        style="box-sizing:border-box;font-variant-numeric:normal;height:32px;padding:0 10px;border-radius:4px;border:1px solid var(--u4-line-ctl);background:{vm.onBg};color:{vm.onText};font-size:13px;cursor:pointer"
        >{vm.onLabel}</button
      >
    </div>
    {#if showType}
      <div style="box-sizing:content-box;display:flex;gap:4px">
        {#each vm.types as t, i (t.label)}
          <button
            onclick={() => onEvent?.('type', { band: vm.band, type: vm.typeValues[i] })}
            aria-pressed={t.pressed}
            aria-label={vm.name + ' ' + t.label}
            style="box-sizing:border-box;font-variant-numeric:normal;flex:1;height:30px;border-radius:4px;border:1px solid var(--u4-line-ctl);background:{t.bg};color:{t.fg};font-size:13px;cursor:pointer"
            >{t.label}</button
          >
        {/each}
      </div>
    {/if}
    <div style="box-sizing:content-box;display:flex;justify-content:space-around;gap:2px">
      <Knob
        specKey="gain"
        mode="eq"
        eqBand={vm.band}
        eqParam="f"
        value={vm.freq}
        label="Freq"
        color={vm.color}
        size={58}
        defaultValue={vm.freq}
        fine={props.fine ?? false}
        oninput={(v) => param('f', v)}
      />
      <Knob
        specKey="gain"
        mode="eq"
        eqBand={vm.band}
        eqParam="g"
        value={vm.gain}
        label="Gain"
        color={vm.color}
        size={58}
        defaultValue={0}
        fine={props.fine ?? false}
        oninput={(v) => param('g', v)}
      />
      <Knob
        specKey="gain"
        mode="eq"
        eqBand={vm.band}
        eqParam="q"
        value={vm.q}
        label="Q"
        color={vm.color}
        size={58}
        defaultValue={vm.q}
        fine={props.fine ?? false}
        oninput={(v) => param('q', v)}
      />
    </div>
  </div>
{/if}

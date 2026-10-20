<!-- W5b `input-preamp-card`: the Input tab Preamp card (gain/trim knobs,
     48V + polarity buttons). Presentational: the InputVm object in via
     layout `bind`, `knob` ({key,value}) / `phantom` / `invert` events out.
     Card chrome + knobs verbatim from design-template.html (isInputTab). -->
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
  aria-label="Preamp"
  style="box-sizing:content-box;display:flex;flex-direction:column;gap:10px;padding:10px;border-radius:3px;border:1px solid var(--u4-line-panel);background:var(--u4-panel-bg)"
>
  <div style="font-size:12px;color:var(--u4-text-6)">Preamp</div>
  <div style="box-sizing:content-box;display:flex;justify-content:space-around;gap:6px">
    <Knob
      specKey="gain"
      value={vm.gain}
      label="Gain"
      color="#7be27b"
      size={68}
      defaultValue={30}
      fine={props.fine ?? false}
      oninput={(v) => onEvent?.('knob', { key: 'gain', value: v })}
    />
    <Knob
      specKey="trim"
      value={vm.trim}
      label="Trim"
      color="#7be27b"
      size={68}
      defaultValue={0}
      fine={props.fine ?? false}
      oninput={(v) => onEvent?.('knob', { key: 'trim', value: v })}
    />
  </div>
  <div style="box-sizing:content-box;display:flex;gap:6px">
      <button
        onclick={() => onEvent?.('phantom')}
        aria-pressed={vm.phantomOn ? 'true' : 'false'}
        style="box-sizing:border-box;font-variant-numeric:normal;flex:1;height:48px;border-radius:3px;border:1px solid {vm.phantomBorder};background:{vm.phantomBg};color:{vm.phantomFg};font-size:14px;font-weight:600;cursor:pointer"
        >{vm.phantomLabel}</button
      >
    <button
      onclick={() => onEvent?.('invert')}
      aria-pressed={vm.invert ? 'true' : 'false'}
      style="box-sizing:border-box;font-variant-numeric:normal;flex:1;height:48px;border-radius:3px;border:1px solid var(--u4-line-ctl);background:{vm.invertBg};color:{vm.invertFg};font-size:14px;font-weight:600;cursor:pointer"
      >Ø Polarity</button
    >
  </div>
</section>
{/if}

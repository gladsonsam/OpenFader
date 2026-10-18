<!-- F1 `fx-detail-panel` module: the detail card below the slot row. L1
      module form: the selected slot + its knob descriptors + header vms in
      via layout `bind` (the module reads the selected slot through its
      props/scope), `toggle` / `tap` / `pickType` {type} / `knob` {param,
      value} / `reset` {param} events out (the layout maps them to ui.toggleFxOn
      / ui.tapFx / ui.setFxType / ui.setFxParam / ui.resetFxParam). The type
      selector is a repeat of buttons (FxTypeSelector over the type list);
      the knob row is descriptor-driven (FxParamKnobs from the caps/fx table
      list). Layout verbatim from the design template: col, gap 16, padding
      14, radius 3, border #1f2836, bg #0e1520; header row (title 20px/600 +
      tap + on buttons h 44); type grid; knob row. -->
<script lang="ts">
  import FxTypeSelector from './FxTypeSelector.svelte';
  import FxParamKnobs from './FxParamKnobs.svelte';
  import type { ModuleContext } from '../../modules/types.ts';
  import type { FxSlot } from '../../fixtures/designData.ts';
  import type {
    FxKnobParamDef,
    FxToggleVm,
    FxTypeVm,
  } from './fxModel.ts';

  let {
    props = {},
    ctx,
    nodeId,
    onEvent,
  }: {
    props: {
      title?: string;
      toggle?: FxToggleVm;
      isDelay?: boolean;
      tapLabel?: string;
      types?: FxTypeVm[];
      slot?: FxSlot;
      knobs?: FxKnobParamDef[];
      fine?: boolean;
      locked?: boolean;
    };
    ctx: ModuleContext;
    nodeId: string;
    onEvent?: (name: string, payload?: unknown) => void;
  } = $props();

  const toggle = $derived(
    props.toggle ?? { label: '', pressed: false, bg: '', fg: '', border: '' },
  );
</script>

<div
  style="box-sizing:content-box;flex:1 1 auto;min-height:300px;display:flex;flex-direction:column;gap:16px;padding:14px;border-radius:3px;border:1px solid #1f2836;background:#0e1520"
>
  <div style="box-sizing:content-box;display:flex;flex-wrap:wrap;align-items:center;gap:8px">
    <div style="font-size:20px;font-weight:600;margin-right:auto"><span>{props.title ?? ''}</span></div>
    {#if props.isDelay}
      <button
        onclick={() => onEvent?.('tap')}
        style="box-sizing:border-box;font-variant-numeric:normal;height:44px;padding:0 18px;border-radius:3px;border:1px solid #3a414c;background:#2b3038;color:#e8edf5;font-size:14px;cursor:pointer"
        >{props.tapLabel ?? ''}</button
      >
    {/if}
    <button
      onclick={() => onEvent?.('toggle')}
      aria-pressed={toggle.pressed ? 'true' : 'false'}
      style="box-sizing:border-box;font-variant-numeric:normal;height:44px;padding:0 18px;border-radius:3px;border:1px solid {toggle.border};background:{toggle.bg};color:{toggle.fg};font-size:14px;cursor:pointer"
      >{toggle.label}</button
    >
  </div>
  <FxTypeSelector
    props={{ types: props.types ?? [] }}
    {ctx}
    nodeId={nodeId + '/types'}
    onEvent={(name, payload) => {
      if (name === 'pickType') onEvent?.('pickType', payload);
    }}
  />
  {#if props.slot}
    <FxParamKnobs
      props={{
        slot: props.slot,
        knobs: props.knobs ?? [],
        fine: props.fine ?? false,
        locked: props.locked ?? false,
      }}
      {ctx}
      nodeId={nodeId + '/knobs'}
      onEvent={(name, payload) => {
        if (name === 'knob') onEvent?.('knob', payload);
        else if (name === 'reset') onEvent?.('reset', payload);
      }}
    />
  {/if}
</div>

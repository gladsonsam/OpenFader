<!-- F1 module `fx-param-knobs`: the descriptor-driven knob row. L1 module
      form: the selected slot + knob descriptors (data: FX_PARAM_DEFS so any
      desk's FX types render without code changes) + fine/locked in via layout
      `bind`, `knob` {param, value} / `reset` {param} events out (the layout
      maps them to ui.setFxParam / ui.resetFxParam). Knobs are the P1 Knob
      primitive in norm mode (92px FX style, design knobItem geometry, 180px
      full-scale drag, fine 0.25, double-tap reset to 0.5, keyboard): hot-path
      drags stay local-first inside Knob and only commit events leave.
      Knobs whose param cap is absent are removed (honest UI): the bound
      descriptor list is pre-filtered (caps, or all); the module only renders
      what it is given. -->
<script lang="ts">
  import Knob from '../../primitives/signal/Knob.svelte';
  import type { ModuleContext } from '../../modules/types.ts';
  import { fxKnobDefs, type FxSlot } from '../../fixtures/designData.ts';
  import { FX_ACCENT, type FxKnobParamDef } from './fxModel.ts';

  let {
    props = {},
    ctx: _ctx,
    nodeId: _nodeId,
    onEvent,
  }: {
    props: {
      slot?: FxSlot;
      knobs?: FxKnobParamDef[];
      fine?: boolean;
      locked?: boolean;
    };
    ctx: ModuleContext;
    nodeId: string;
    onEvent?: (name: string, payload?: unknown) => void;
  } = $props();

  const slot = $derived(props.slot);
  const knobs = $derived(props.knobs ?? []);
  const defs = $derived(slot ? fxKnobDefs(slot.type) : []);
</script>

<div
  style="box-sizing:content-box;flex:1;display:flex;flex-wrap:wrap;justify-content:space-around;align-items:center;gap:16px"
>
  {#if slot}
    {#each knobs as k, pi (k.key)}
      <Knob
        specKey="gain"
        mode="norm"
        value={slot.p[pi]}
        label={k.label}
        color={FX_ACCENT}
        size={92}
        fx
        defaultValue={0.5}
        fine={props.fine ?? false}
        disabled={props.locked ?? false}
        fmt={(v) => defs[pi].fmt(v)}
        oninput={(v) => onEvent?.('knob', { param: pi, value: v })}
      />
    {/each}
  {/if}
</div>

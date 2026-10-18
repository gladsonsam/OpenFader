<!-- W5b `dyn-knob-panel` module: the gate/compressor 3-column knob grid.
     Presentational: kind + live gate/comp params in, the knob descriptor
     list (`knobs` prop, data from DYN_KNOBS so users can add/remove knobs)
     selects the grid; per-knob `knob` ({prop,value}) events out. Knobs are
     the P1 Knob primitive in spec mode (80px, design knobItem geometry,
     180px full-scale drag, fine 0.25, double-tap reset, keyboard). Panel
     grid verbatim from design-template.html (isDynTab knob card). Knobs
     whose param cap is absent are removed (honest UI): pass `caps`, or null
     for all. -->
<script lang="ts">
  import Knob from '../../../primitives/signal/Knob.svelte';
  import { DYN_KNOBS, dynKnobValue, type DynKind, type DynKnobDef } from './dynModel.ts';
  import type { ModuleContext } from '../../../modules/types.ts';
  import type { Comp, Gate } from '../../../fixtures/designData.ts';

  let {
    props = {},
    ctx: _ctx,
    nodeId: _nodeId,
    onEvent,
  }: {
    props: {
      kind?: DynKind;
      color?: string;
      /** Knob diameter in px (desktop layouts widen it; phone narrows it). */
      size?: number;
      /** Knob descriptor list (data: users add/remove knobs by editing it). */
      knobs?: DynKnobDef[] | null;
      gate?: Gate | null;
      comp?: Comp | null;
      fine?: boolean;
      locked?: boolean;
      /** Caps the desk supports; knobs whose bind is absent are removed. Null = all present. */
      caps?: string[] | null;
    };
    ctx: ModuleContext;
    nodeId: string;
    onEvent?: (name: string, payload?: unknown) => void;
  } = $props();

  const kind = $derived(props.kind ?? 'comp');
  const defs = $derived(props.knobs ?? DYN_KNOBS[kind]);
  const visible = $derived(
    props.caps == null ? defs : defs.filter((k) => props.caps!.includes(k.bind)),
  );
  const gate = $derived(props.gate ?? null);
  const comp = $derived(props.comp ?? null);
</script>

<div
  style="box-sizing:content-box;flex:1 1 300px;display:grid;grid-template-columns:repeat(3, minmax(0, 1fr));align-content:center;gap:20px 8px;padding:18px;border-radius:3px;border:1px solid var(--u4-line-panel);background:var(--u4-panel-bg)"
>
  {#if gate && comp}
    {#each visible as k (k.prop)}
      <Knob
        specKey={k.specKey}
        value={dynKnobValue(kind, gate, comp, k.prop)}
        label={k.label}
        color={props.color ?? '#ff9f43'}
        size={props.size ?? 80}
        defaultValue={k.defaultValue}
        fine={props.fine ?? false}
        disabled={props.locked ?? false}
        oninput={(v) => onEvent?.('knob', { prop: k.prop, value: v })}
      />
    {/each}
  {/if}
</div>

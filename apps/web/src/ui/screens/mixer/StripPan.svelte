<!-- W5c module `strip-pan`: one strip's pan bar (ModuleProps wrapper over the
     P1 PanBar). Value null (DCA strips) renders the design's dark placeholder
     well verbatim from design-template.html (content-box, so the 30px height
     + border matches to the px). Drags stay local-first inside PanBar; the
     committed value emits `input` (payload.value) for the layout `on` action
     (ui.setPan, lock-checked in the store). -->
<script lang="ts">
  import PanBar from '../../primitives/signal/PanBar.svelte';
  import type { ModuleContext } from '../../modules/types.ts';

  let {
    props = {},
    ctx: _ctx,
    nodeId: _nodeId,
    onEvent,
  }: {
    props: {
      value?: number | null;
      name?: string;
      variant?: 'strip' | 'aside';
      disabled?: boolean;
      fine?: boolean;
    };
    ctx: ModuleContext;
    nodeId: string;
    onEvent?: (name: string, payload?: unknown) => void;
  } = $props();

  const value = $derived(props.value ?? null);
</script>

{#if value == null}
  <div
    aria-hidden="true"
    data-ctl="pan-empty"
    style="height:30px;flex:none;border:1px solid #1a2230;border-radius:3px;box-sizing:content-box"
  ></div>
{:else}
  <PanBar
    value={value}
    name={props.name ?? ''}
    variant={props.variant ?? 'strip'}
    disabled={props.disabled ?? false}
    fine={props.fine ?? false}
    oninput={(v) => onEvent?.('input', { value: v })}
  />
{/if}

<!-- W5c module `strip-pan`: one strip's pan bar (ModuleProps wrapper over the
     P1 PanBar). Value null (DCA strips) renders the design's dark placeholder
     well with the same 30px border-box height as PanBar so mute stays
     aligned across strips. Drags stay local-first inside PanBar; the
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
    style="height:30px;flex:none;border:1px solid var(--u4-rail-border);border-radius:3px;box-sizing:border-box"
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

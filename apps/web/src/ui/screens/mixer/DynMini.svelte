<script lang="ts">
  import { onMount } from 'svelte';
  import type { ModuleContext } from '../../modules/types.ts';

  let { props = {}, ctx, onEvent }: {
    props: {
      stripId?: string;
      kind?: 'gate' | 'comp';
      enabled?: boolean;
      ariaLabel?: string;
      label?: string;
    };
    ctx: ModuleContext;
    nodeId: string;
    onEvent?: (name: string, payload?: unknown) => void;
  } = $props();

  let reduction = $state<number | null>(null);
  onMount(() => {
    const update = () => {
      reduction = props.enabled
        ? ctx.useMeter(`meter:${props.kind ?? 'comp'}/${props.stripId ?? ''}`)
        : 0;
    };
    update();
    const timer = setInterval(update, 80);
    return () => clearInterval(timer);
  });
  const amount = $derived(Math.min(100, Math.max(0, (reduction ?? 0) / 30 * 100)));
</script>

<button
  onclick={() => onEvent?.('open')}
  aria-label={props.ariaLabel ?? 'Open dynamics'}
  title={`${props.label ?? 'Dynamics'}: ${!props.enabled ? 'Off' : reduction === null ? 'No reduction data' : reduction.toFixed(1) + ' dB reduction'}`}
  style="flex:none;height:24px;min-height:24px;padding:5px 4px;border:1px solid var(--u4-line-panel);border-radius:3px;background:var(--u4-well-bg-2);cursor:pointer;overflow:hidden;text-align:left"
>
  <span aria-hidden="true" style="display:block;height:10px;border:1px solid var(--u4-line-panel);border-radius:1px;background:var(--u4-well-bg);overflow:hidden">
    <span style={`display:block;height:100%;width:${amount}%;background:repeating-linear-gradient(to right,#ef4444 0px,#ef4444 4px,transparent 4px,transparent 5px);transition:width 80ms linear`}></span>
  </span>
</button>

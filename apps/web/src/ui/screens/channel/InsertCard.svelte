<!-- `channel-insert-card`: per-strip FX-insert bypass + slot select for
      the selected channel (both caps-gated: X-Air ch/bus/main verified,
      sim catalogue-wide).

      Presentational: the InsertVm object in via layout `bind`, bypass toggle
      and select stepper commits out via `onEvent` (`toggle` {on}, `sel`
      {value}). Live-only: the layout `when` hides the card offline, so
      fixture mode and the fixture baselines are untouched. Unknown
      (not-yet-echoed) states render as `?` — never faked. The select is the
      desk's raw 0..8 int (NOT 1..4 FX slots); the desk exposes no insert
      position, so none is shown. -->
<script lang="ts">
  import type { ModuleContext } from '../../modules/types.ts';
  import type { InsertVm } from './insertModel.ts';

  let {
    props = {},
    ctx: _ctx,
    nodeId: _nodeId,
    onEvent,
  }: {
    props: { vm?: InsertVm | null };
    ctx: ModuleContext;
    nodeId: string;
    onEvent?: (name: string, payload?: unknown) => void;
  } = $props();

  const vm = $derived(props.vm ?? null);
</script>

{#if vm}
  <div
    style="box-sizing:content-box;flex:1 1 auto;min-height:120px;overflow-y:auto;display:flex;flex-direction:column;gap:6px;padding:10px;border-radius:3px;border:1px solid #1f2836;background:#0a0f17"
  >
    <span style="font-size:15px;font-weight:600;color:#eef2f7;white-space:nowrap;overflow:hidden;text-overflow:ellipsis"
      >{vm.title}</span
    >
    {#if vm.onSupported}
      <div style="display:flex;flex-direction:column;gap:4px">
        <span style="font-size:12px;color:#7f8a9c">Bypass</span>
        <button
          onclick={() => onEvent?.('toggle', { on: !(vm.on === true) })}
          aria-pressed={vm.on === true ? 'true' : 'false'}
          aria-label={vm.onAria}
          title={vm.on === null ? 'Insert bypass not confirmed yet — tap to set' : undefined}
          style="box-sizing:border-box;font-variant-numeric:normal;width:100%;height:40px;flex:none;border-radius:3px;border:1px solid #3a414c;background:{vm.onBg};color:{vm.onFg};font-size:12px;font-weight:600;cursor:pointer"
          >{vm.onLabel}</button
        >
      </div>
    {/if}
    {#if vm.selSupported}
      <div style="display:flex;flex-direction:column;gap:4px">
        <span style="font-size:12px;color:#7f8a9c">Select · {vm.selText}</span>
        <div style="display:flex;gap:6px">
          <button
            onclick={() => onEvent?.('sel', { value: (vm.sel ?? 0) - 1 })}
            aria-label={vm.selMinusAria}
            title={vm.sel === null ? 'Insert select not confirmed yet — tap to set' : undefined}
            style="flex:1;height:40px;border-radius:3px;border:1px solid #3a414c;background:#2b3038;color:#c9d1dc;font-size:16px;font-weight:600;cursor:pointer"
            >−</button
          >
          <button
            onclick={() => onEvent?.('sel', { value: (vm.sel ?? 0) + 1 })}
            aria-label={vm.selPlusAria}
            title={vm.sel === null ? 'Insert select not confirmed yet — tap to set' : undefined}
            style="flex:1;height:40px;border-radius:3px;border:1px solid #3a414c;background:#2b3038;color:#c9d1dc;font-size:16px;font-weight:600;cursor:pointer"
            >+</button
          >
        </div>
      </div>
    {/if}
    {#if vm.note}
      <div style="font-size:12px;color:#7f8a9c">{vm.note}</div>
    {/if}
  </div>
{/if}

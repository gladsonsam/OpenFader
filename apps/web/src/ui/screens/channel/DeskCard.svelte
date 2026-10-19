<!-- `channel-desk-card`: desk LR assign + automix weight for the selected
     channel (both caps-gated: X-Air verified, sim catalogue-wide).

     Presentational: the DeskVm object in via layout `bind`, LR toggle and
     weight slider commits out via `onEvent` (`lr` {assigned}, `automix`
     {value}). Live-only: the layout `when` hides the card offline, so
     fixture mode and the fixture baselines are untouched. Unknown
     (not-yet-echoed) states render as `?` / dim — never faked. -->
<script lang="ts">
  import type { ModuleContext } from '../../modules/types.ts';
  import type { DeskVm } from './deskModel.ts';

  let {
    props = {},
    ctx: _ctx,
    nodeId: _nodeId,
    onEvent,
  }: {
    props: { vm?: DeskVm | null };
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
    {#if vm.lrSupported}
      <div style="display:flex;flex-direction:column;gap:4px">
        <span style="font-size:12px;color:#7f8a9c">Main LR</span>
        <button
          onclick={() => onEvent?.('lr', { assigned: !(vm.lrAssigned === true) })}
          aria-pressed={vm.lrAssigned === true ? 'true' : 'false'}
          aria-label={vm.lrAria}
          title={vm.lrAssigned === null ? 'LR assign not confirmed yet — tap to set' : undefined}
          style="box-sizing:border-box;font-variant-numeric:normal;width:100%;height:40px;flex:none;border-radius:3px;border:1px solid #3a414c;background:{vm.lrBg};color:{vm.lrFg};font-size:12px;font-weight:600;cursor:pointer"
          >{vm.lrLabel}</button
        >
      </div>
    {/if}
    {#if vm.automixSupported}
      <div style="display:flex;flex-direction:column;gap:4px">
        <span style="font-size:12px;color:#7f8a9c">Automix weight · {vm.automixText}</span>
        <input
          type="range"
          min="0"
          max="100"
          step="1"
          value={vm.automixWeight === null ? 50 : Math.round(vm.automixWeight * 100)}
          aria-label={vm.automixAria}
          title={vm.automixWeight === null ? 'Weight not confirmed yet — drag to set' : undefined}
          onchange={e => onEvent?.('automix', { value: Number((e.currentTarget as HTMLInputElement).value) / 100 })}
          style="width:100%;height:40px;accent-color:#5fe3e0;cursor:pointer"
        />
      </div>
    {/if}
    {#if vm.note}
      <div style="font-size:12px;color:#7f8a9c">{vm.note}</div>
    {/if}
  </div>
{/if}

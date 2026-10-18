<!-- L1 module `device-row`: one connection device row.
     Presentational: whole row object in via layout `bind`, `connect` event
     out (payload carries the device key; the JSON maps it to ui.connect).
     Inline styles + icon paths verbatim from the design template. -->
<script lang="ts">
  import type { ModuleContext } from '../../modules/types.ts';
  import type { DeviceVm } from './connectionVm.ts';

  let {
    props = {},
    ctx: _ctx,
    nodeId: _nodeId,
    onEvent,
  }: {
    props: { device?: DeviceVm };
    ctx: ModuleContext;
    nodeId: string;
    onEvent?: (name: string, payload?: unknown) => void;
  } = $props();

  const d = $derived(
    props.device ?? {
      key: '',
      name: '',
      sub: '',
      icon: '',
      status: '',
      statusColor: 'var(--u4-text-7)',
      border: 'var(--u4-line-panel)',
      canConnect: false,
      btnLabel: '',
      btnBg: '#1a2a1b',
      btnText: '#7be27b',
      btnBorder: '#2c4a2e',
    },
  );
</script>

<div
  style="box-sizing:content-box;display:flex;align-items:center;gap:14px;padding:12px 14px;border-radius:3px;border:1px solid {d.border};background:var(--u4-panel-bg)"
>
  <div
    style="box-sizing:content-box;width:44px;height:44px;flex:none;display:flex;align-items:center;justify-content:center;border-radius:3px;background:#1a2332;color:var(--u4-text-4)"
  >
    <svg
      width="24"
      height="24"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="1.8"
      stroke-linecap="round"
      stroke-linejoin="round"
      aria-hidden="true"><path d={d.icon}></path></svg
    >
  </div>
  <div style="box-sizing:content-box;flex:1;min-width:0">
    <div style="box-sizing:content-box;font-size:16px;font-weight:600">{d.name}</div>
    <div
      style="box-sizing:content-box;font-size:12px;color:var(--u4-text-7);white-space:nowrap;overflow:hidden;text-overflow:ellipsis"
    >{d.sub}</div>
  </div>
  <span style="font-size:12px;font-weight:600;color:{d.statusColor};white-space:nowrap">{d.status}</span>
  {#if d.canConnect}
    <button
      onclick={() => onEvent?.('connect', { key: d.key })}
      style="box-sizing:border-box;font-variant-numeric:normal;height:44px;min-width:104px;padding:0 14px;border-radius:3px;border:1px solid {d.btnBorder};background:{d.btnBg};color:{d.btnText};font-size:14px;cursor:pointer"
    >{d.btnLabel}</button>
  {/if}
</div>

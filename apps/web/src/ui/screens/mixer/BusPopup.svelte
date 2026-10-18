<!-- Bus selector popup (item 7): tapping the MAIN LR master opens the
     available-bus list (Main LR + bus masters + FX returns, names +
     mini-meters, active highlighted). Tap a row to switch master/bank,
     tap the backdrop or ✕ to close. Presentational: rows flow in, picks
     dispatch onPick(id) / onClose. -->
<script lang="ts">
  export interface BusRowVm {
    id: string;
    name: string;
    sub: string;
    /** Mute state text (MUTED/open/?). */
    state: string;
    stateColor: string;
    /** Active master row. */
    active: boolean;
    /** Mini-meter fill 0..100 (% text). */
    meterPct: string;
    meterAria: string;
  }

  let {
    rows = [],
    onPick,
    onClose,
  }: {
    rows: BusRowVm[];
    onPick?: (id: string) => void;
    onClose?: () => void;
  } = $props();
</script>

<div
  role="dialog"
  aria-label="Select master bus"
  style="position:fixed;inset:0;z-index:60;display:flex;align-items:center;justify-content:center;background:rgba(4,8,14,0.72);padding:16px"
  onclick={e => {
    if (e.target === e.currentTarget) onClose?.();
  }}
  onkeydown={e => {
    if (e.key === 'Escape') onClose?.();
  }}
>
  <div
    style="box-sizing:content-box;width:min(420px,100%);max-height:min(560px,90vh);overflow-y:auto;display:flex;flex-direction:column;gap:6px;padding:14px;border-radius:6px;border:1px solid #2a3443;background:#0b111b"
  >
    <div style="display:flex;align-items:center;gap:8px">
      <span style="font-size:16px;font-weight:700;color:#eef2f5;margin-right:auto">MASTER</span>
      <button
        onclick={() => onClose?.()}
        aria-label="Close master bus selector"
        style="box-sizing:border-box;width:36px;height:36px;border-radius:3px;border:1px solid #3a414c;background:#2b3038;color:#e8edf5;font-size:16px;cursor:pointer"
        >✕</button
      >
    </div>
    {#each rows as r (r.id)}
      <button
        onclick={() => onPick?.(r.id)}
        aria-label={'Select ' + r.name + (r.active ? ' (current master)' : '')}
        aria-pressed={r.active ? 'true' : 'false'}
        style="box-sizing:border-box;display:flex;align-items:center;gap:10px;min-height:52px;padding:8px 10px;border-radius:3px;border:1px solid {r.active
          ? '#b28cff'
          : '#222a36'};background:{r.active ? '#1b2330' : '#0e1520'};cursor:pointer;text-align:left"
      >
        <span style="flex:none;min-width:92px">
          <span style="display:block;font-size:14px;font-weight:700;color:#eef2f5">{r.name}</span>
          <span style="display:block;font-size:11px;color:#8792a4">{r.sub}</span>
        </span>
        <span style="flex:1 1 auto;min-width:0;height:6px;border-radius:3px;background:#1a2332;overflow:hidden">
          <span
            role="img"
            aria-label={r.meterAria}
            style="display:block;height:100%;width:{r.meterPct}%;background:#b28cff"
          ></span>
        </span>
        <span style="flex:none;font-size:11px;font-weight:700;color:{r.stateColor}">{r.state}</span>
      </button>
    {/each}
  </div>
</div>

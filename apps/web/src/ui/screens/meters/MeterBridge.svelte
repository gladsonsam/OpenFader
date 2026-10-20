<!-- Meter-bridge overlay (backlog: global meter bridge, mixer control software pattern).

     A dialog opened from the topbar meter-bridge button (from anywhere — it
     floats above the shell outlet and never disturbs mode/bank/rail). Grouped
     ladders 0..-50 with bottom scribbles, peak-hold + clip where the engine
     has the data. Levels come ONLY from the live engine path while online;
     offline every ladder is honestly dark (never sim-animated, never fake).
     Groups the desk has no strips for collapse with a reason. The shell body
     never scrolls, so the panel owns an internal vertical scroller (and each
     group scrolls horizontally on narrow viewports). -->
<script lang="ts">
  import { live } from '../../state/live.svelte.ts';
  import { useDeviceClass } from '../../modules/device.svelte.ts';
  import { buildMeterGroups } from './meterBridgeModel.ts';
  import { meterBridge } from './meterBridgeState.svelte.ts';
  import MeterGroupLadders from './MeterGroupLadders.svelte';

  const device = useDeviceClass();
  const isPhone = $derived(device.now === 'phone');

  const online = $derived(live.deskConnected || live.demo);
  const demo = $derived(live.demo);
  const deskName = $derived(
    (() => {
      void live.version;
      const w = live.engine;
      if (live.phase !== 'online' || !w) return null;
      try {
        return w.info?.model ?? w.driver;
      } catch {
        return w.driver;
      }
    })(),
  );
  const groups = $derived(
    (() => {
      void live.version;
      const w = live.engine;
      const up = online;
      return buildMeterGroups(up ? w : null, up, deskName);
    })(),
  );
  const liveCount = $derived(groups.reduce((n, g) => n + g.ladders.length, 0));

  function close(): void {
    meterBridge.close();
  }
</script>

<!-- svelte-ignore a11y_interactive_supports_focus -->
<div
  role="dialog"
  aria-modal="true"
  aria-label="Meter bridge"
  style="position:fixed;inset:0;top:var(--app-offset-top,0px);bottom:auto;height:var(--app-height,100dvh);box-sizing:border-box;z-index:60;display:flex;justify-content:flex-end;background:rgba(4,8,14,0.72);{isPhone ? 'padding:var(--safe-area-top,env(safe-area-inset-top,0px)) var(--safe-area-right,env(safe-area-inset-right,0px)) var(--safe-area-bottom,env(safe-area-inset-bottom,0px)) var(--safe-area-left,env(safe-area-inset-left,0px))' : 'padding:16px'}"
  onclick={e => {
    if (e.target === e.currentTarget) close();
  }}
  onkeydown={e => {
    if (e.key === 'Escape') close();
  }}
>
  <div
    style="box-sizing:border-box;display:flex;flex-direction:column;min-height:0;background:#0b111b;border:1px solid #2a3443;{isPhone
      ? 'width:100%;height:100%;border:none'
      : 'width:min(1060px,94vw);max-height:92vh;border-radius:8px'}"
  >
    <div style="flex:none;display:flex;align-items:center;gap:8px;padding:10px 12px;border-bottom:1px solid #2a3443">
      <span style="font-size:15px;font-weight:700;letter-spacing:.04em;color:#eef2f5;margin-right:auto">METERS</span>
      {#if online}
        <span
          style="font-size:11px;font-weight:700;letter-spacing:.06em;color:#0b111b;background:{demo ? '#f0b429' : '#5fe060'};border-radius:3px;padding:3px 8px"
          >{demo ? 'DEMO' : 'LIVE · ' + deskName}</span
        >
      {:else}
        <span
          style="font-size:11px;font-weight:700;letter-spacing:.06em;color:#0b111b;background:#f0b429;border-radius:3px;padding:3px 8px"
          >OFFLINE</span
        >
      {/if}
      <span style="font-size:11px;color:#7f8a9c">{liveCount} ladders</span>
      <!-- svelte-ignore a11y_autofocus -->
      <button
        onclick={close}
        aria-label="Close meter bridge"
        autofocus
        style="box-sizing:border-box;width:44px;height:44px;flex:none;border-radius:3px;border:1px solid #3a414c;background:#2b3038;color:#e8edf5;font-size:16px;cursor:pointer"
        >✕</button
      >
    </div>

    <div style="flex:1;min-height:0;overflow-y:auto;padding:10px 12px 16px;display:flex;flex-direction:column;gap:14px">
      {#if !online}
        <div style="font-size:12px;color:#aeb8c4;background:#111927;border:1px solid #2a323f;border-radius:4px;padding:8px 10px">
          Offline — ladders are dark until a live link is up. Connect for live meters.
        </div>
      {/if}
      {#each groups as g (g.key)}
        {#if g.ladders.length}
          <section aria-label={g.title} style="background:#0d141d;border:1px solid #232c3a;border-radius:6px;padding:8px 10px 10px">
            <div style="display:flex;align-items:baseline;gap:8px;margin-bottom:6px">
              <h2 style="margin:0;font-size:12px;font-weight:700;letter-spacing:.05em;color:#e8edf5">{g.title}</h2>
              {#if !online}
                <span style="font-size:10px;color:#7f8a9c">no data</span>
              {/if}
            </div>
            <MeterGroupLadders group={g} world={online ? live.engine : null} {online} />
          </section>
        {:else}
          <section
            aria-label={g.title + ' collapsed'}
            style="display:flex;align-items:center;gap:10px;background:#0d141d;border:1px solid #1b2330;border-radius:6px;padding:7px 10px;opacity:0.75"
          >
            <h2 style="margin:0;font-size:12px;font-weight:600;letter-spacing:.05em;color:#7f8a9c;white-space:nowrap">{g.title}</h2>
            <span style="font-size:11px;color:#5b6572;white-space:nowrap;overflow:hidden;text-overflow:ellipsis">
              {g.collapsedReason === 'offline' ? 'no data (offline)' : g.collapsedReason}
            </span>
          </section>
        {/if}
      {/each}
    </div>
  </div>
</div>

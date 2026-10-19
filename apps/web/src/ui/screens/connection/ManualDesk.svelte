<!-- L1 module `manual-desk`: add-by-IP for desks broadcast discovery cannot
     see (cross-subnet: e.g. the app on 192.168.1.x while the XR18 sits on
     192.168.40.x). The relay itself pings the typed address with a directed
     unicast probe — verified rows appear in the device list as connectable
     rows (`via: manual`, tap-to-connect like any discovered desk); silence
     stays an honest inline hint, never a blind row. Recent hosts persist
     (localStorage) as one-tap chips. Probe sending + persistence live in
     `ui.probeDesk` (uiStore -> live.rememberDesk + engine probe); this card
     only emits `probe-desk` (payload carries the typed host) and re-reads
     recents after the (synchronous) dispatch. -->
<script lang="ts">
  import { live } from '../../state/live.svelte.ts';

  let {
    onEvent,
  }: {
    onEvent?: (name: string, payload?: unknown) => void;
  } = $props();

  function bareIp(v: string): string {
    const s = v.trim();
    const idx = s.lastIndexOf(':');
    if (idx > 0 && /^\d+$/.test(s.slice(idx + 1))) return s.slice(0, idx).replace(/^\[|\]$/g, '');
    return s.replace(/^\[|\]$/g, '');
  }

  let draft = $state('');
  let recents = $state<string[]>(live.recentDesks());
  let pending = $state<string | null>(null);
  let knownAtTap = $state<Set<string>>(new Set());
  let timedOut = $state(false);
  let timer: ReturnType<typeof setTimeout> | null = $state(null);

  const online = $derived(live.phase === 'online');
  const devices = $derived(
    // Re-read on every engine ingest (the probe answer lands as a devices
    // frame; without this the found-row check stays stale until the next
    // local state change). Same pattern as ConnectionScreen's vm.
    ((): Array<{ id: string; ip: string; via?: string | null }> => {
      void live.version;
      return live.engine?.devices ?? [];
    })(),
  );
  const found = $derived(
    pending !== null &&
      devices.some(d => d.ip === bareIp(pending!) || (d.via === 'manual' && !knownAtTap.has(d.id))),
  );
  const status = $derived(
    !online
      ? 'Relay link is down — it reconnects on its own; once online, Add pings the desk.'
      : pending === null
        ? 'Desks on another subnet never answer broadcast. Type the desk IP — the relay pings it directly.'
        : found
          ? `${pending} answered — use Connect on its row above.`
          : timedOut
            ? `No reply from ${pending} — check the IP and that the desk is on.`
            : `Pinging ${pending} via the relay …`,
  );

  function clearTimer(): void {
    if (timer !== null) {
      clearTimeout(timer);
      timer = null;
    }
  }

  function add(raw: string): void {
    const v = raw.trim();
    if (!v || !online) return;
    clearTimer();
    knownAtTap = new Set(devices.map(d => d.id));
    pending = v;
    timedOut = false;
    if (draft !== v) draft = v;
    // ui.probeDesk persists the host (recents) + sends the engine probe.
    onEvent?.('probe-desk', { ip: v });
    recents = live.recentDesks();
    const token = v;
    timer = setTimeout(() => {
      timer = null;
      if (pending === token) timedOut = true;
    }, 6000);
  }
</script>

<div
  style="box-sizing:content-box;display:flex;flex-direction:column;gap:8px;padding:12px 14px;border-radius:3px;border:1px dashed var(--u4-line-ctl);background:var(--u4-well-bg)"
>
  <div style="box-sizing:content-box;font-size:13px;font-weight:600">Add desk by IP</div>
  <div style="box-sizing:content-box;display:flex;gap:8px;flex-wrap:wrap">
    <input
      bind:value={draft}
      aria-label="Desk IP address"
      placeholder="192.168.40.123"
      autocapitalize="off"
      autocomplete="off"
      autocorrect="off"
      spellcheck={false}
      inputmode="decimal"
      onkeydown={e => {
        if (e.key === 'Enter') add(draft);
      }}
      style="box-sizing:border-box;flex:1 1 140px;min-width:0;height:44px;padding:0 12px;border-radius:3px;border:1px solid var(--u4-line-ctl);background:var(--u4-panel-bg);color:var(--u4-text-1);font-size:16px;font-family:inherit"
    />
    <button
      onclick={() => add(draft)}
      disabled={!online || !draft.trim()}
      aria-label="Ping desk IP via relay"
      title={online ? 'Ask the relay to ping this address' : 'Relay link is down'}
      style="box-sizing:border-box;height:44px;min-width:104px;padding:0 14px;border-radius:3px;border:1px solid var(--u4-line-ctl);background:var(--u4-btn-bg);color:var(--u4-text-1);font-size:14px;cursor:pointer;font-variant-numeric:normal"
      >Add</button
    >
  </div>
  {#if recents.length > 0}
    <div style="box-sizing:content-box;display:flex;gap:6px;flex-wrap:wrap">
      {#each recents as r (r)}
        <button
          onclick={() => add(r)}
          disabled={!online}
          aria-label="Re-probe {r}"
          style="box-sizing:border-box;min-height:32px;padding:2px 10px;border-radius:3px;border:1px solid var(--u4-line-ctl);background:transparent;color:var(--u4-text-5);font-size:12px;cursor:pointer;font-variant-numeric:normal"
          >{r}</button
        >
      {/each}
    </div>
  {/if}
  <div style="box-sizing:content-box;font-size:12px;color:var(--u4-text-7)" role="status">{status}</div>
</div>

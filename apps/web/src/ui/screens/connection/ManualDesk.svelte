<!-- L1 module `manual-desk`: add-by-IP for desks broadcast discovery cannot
     see (cross-subnet: e.g. the app on 192.168.1.x while the XR18 sits on
     192.168.40.x). Add does the whole job: the relay pings the typed address
     with a directed unicast probe and, when the desk answers, CONNECTS to it
     immediately (same path as tapping Connect on a scanned row) — the
     verified row appears as `via: manual` and the link flips to Connected.
     Silence stays an honest inline hint, never a blind row. Recent hosts
     persist (localStorage) as one-tap chips. Probe + connect happen in the
     engine; this card only emits `probe-desk` (payload carries the typed
     host) and re-reads recents + the live link. -->
<script lang="ts">
  import { onDestroy } from 'svelte';
  import { live } from '../../state/live.svelte.ts';

  let {
    onEvent,
  }: {
    onEvent?: (name: string, payload?: unknown) => void;
  } = $props();
  const fieldId = $props.id();

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
    ((): Array<{ id: string; ip: string; name?: string | null; via?: string | null }> => {
      void live.version;
      return live.engine?.devices ?? [];
    })(),
  );
  // Rows that appeared since the Add tap: the probe's verified desk (the
  // engine lists it, then connects). Also covers hostname input, where the
  // typed string is not the resolved IP the link reports.
  const pendingIp = $derived(pending === null ? null : bareIp(pending));
  const newManualRows = $derived(
    devices.filter(d => d.via === 'manual' && !knownAtTap.has(d.id)),
  );
  const found = $derived(
    pending !== null && (devices.some(d => d.ip === pendingIp) || newManualRows.length > 0),
  );
  // The verified desk is connected by the engine right after the probe
  // answers: match the live link against the pending address (ip, or the
  // name of the manual row the probe just added).
  const link = $derived((() => {
    void live.version;
    return live.engine?.link ?? null;
  })());
  const connectedName = $derived(
    pending !== null &&
      link !== null &&
      link.state === 'connected' &&
      (link.ip === pendingIp || newManualRows.some(d => !!link.name && d.name === link.name))
      ? link.name || pending
      : null,
  );
  const status = $derived(
    !online
      ? 'Connection service unavailable. Reconnecting automatically…'
      : pending === null
        ? 'Enter the mixer’s address, then select Connect.'
        : connectedName !== null
          ? `${pending} answered — connected to ${connectedName || pending}.`
          : found
            ? `${pending} answered — connecting…`
            : timedOut
              ? `No reply from ${pending} — check the IP and that the desk is on.`
              : `Checking ${pending}, then connecting…`,
  );

  function clearTimer(): void {
    if (timer !== null) {
      clearTimeout(timer);
      timer = null;
    }
  }
  onDestroy(clearTimer);

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

<section class="manual-desk" aria-labelledby={`${fieldId}-title`}>
  <h2 id={`${fieldId}-title`}>Connect by IP address</h2>
  <p>Use the mixer’s address if it doesn’t appear in a network scan.</p>
  <form onsubmit={e => { e.preventDefault(); add(draft); }}>
    <label for={fieldId}>Mixer IP address</label>
    <p id={`${fieldId}-example`} class="example">Example: 192.168.40.123</p>
    <div class="address-row">
      <input
        id={fieldId}
        bind:value={draft}
        aria-describedby={`${fieldId}-example`}
        autocapitalize="off"
        autocomplete="off"
        autocorrect="off"
        spellcheck={false}
        inputmode="decimal"
      />
      <button type="submit" disabled={!online || !draft.trim()}>Connect</button>
    </div>
  </form>
  {#if recents.length > 0}
    <div class="recents">
      <p>Recent addresses · tap to connect</p>
      <div class="recent-addresses">
        {#each recents as r (r)}
          <button onclick={() => add(r)} disabled={!online} aria-label="Connect to {r}">{r}</button>
        {/each}
      </div>
    </div>
  {/if}
  <p class="status" role="status">{status}</p>
</section>

<style>
  .manual-desk {
    box-sizing: border-box;
    display: flex;
    flex-direction: column;
    gap: 12px;
    min-width: 0;
    padding: 16px;
    border: 1px solid var(--u4-line-panel);
    border-radius: 3px;
    background: var(--u4-panel-bg);
  }
  h2 { margin: 0; font-size: 16px; font-weight: 600; }
  p { margin: 0; color: var(--u4-text-5); font-size: 13px; line-height: 1.5; }
  form { display: flex; flex-direction: column; gap: 6px; }
  label { font-size: 13px; font-weight: 600; }
  .example, .status, .recents p { color: var(--u4-text-7); font-size: 12px; }
  .address-row, .recent-addresses { display: flex; gap: 8px; flex-wrap: wrap; }
  input {
    box-sizing: border-box;
    flex: 1 1 140px;
    min-width: 0;
    height: 44px;
    padding: 0 12px;
    border: 1px solid var(--u4-line-ctl);
    border-radius: 3px;
    background: var(--u4-well-bg);
    color: var(--u4-text-1);
    font: inherit;
    font-size: 16px;
  }
  button {
    box-sizing: border-box;
    min-height: 44px;
    padding: 0 14px;
    border: 1px solid var(--u4-line-ctl);
    border-radius: 3px;
    background: var(--u4-btn-bg);
    color: var(--u4-text-1);
    font: inherit;
    font-size: 14px;
    cursor: pointer;
  }
  button:disabled { opacity: 0.45; cursor: default; }
  input:focus-visible, button:focus-visible { outline: 2px solid var(--u4-green); outline-offset: 2px; }
  .recents { display: flex; flex-direction: column; gap: 6px; }
  .recent-addresses button { padding: 0 10px; background: var(--u4-well-bg); color: var(--u4-text-5); font-size: 12px; }
</style>

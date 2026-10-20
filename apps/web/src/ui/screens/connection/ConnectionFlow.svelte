<script lang="ts">
  import { ui } from '../../state/uiStore.svelte.ts';
  import { live } from '../../state/live.svelte.ts';
  import { buildConnectionVm } from './connectionVm.ts';

  const fieldId = $props.id();
  const offline = $derived(ui.state.connected === 'offline');
  const session = $derived(live.connectionStatus(offline));
  const vm = $derived.by(() => {
    void live.version;
    const w = live.engine;
    return buildConnectionVm(ui.state, {
      phase: live.phase, message: live.message, host: live.host,
      latencyMs: live.latencyMs, strips: w?.stripCount ?? 0,
      capsReady: w?.capsReady ?? false, link: w?.link ?? null,
      devices: w?.devices, scannedAt: w?.scannedAt ?? 0,
      scanning: live.scanning, scanError: live.scanError, driver: w?.driver ?? '',
    });
  });
  const flow = $derived(ui.connectionFlow);
  const pending = $derived(flow.attempt);
  const error = $derived(flow.error || pending?.error);
  const busy = $derived(!!pending || flow.cancelling || flow.scanPending || vm.scanBusy || flow.cleanupRequired);
  const recents = $derived.by(() => { void live.version; return live.recentDesks(); });
  function manual(raw = ui.connectionAddress) { ui.do.probeDesk(raw); }
  function connect(key: string) { ui.do.connect(key); }
  function cancel() { ui.do.cancelConnection(); }
  function scan() { ui.do.scan(new Date().toLocaleTimeString()); }
  async function retryService() { await live.disconnect(); await ui.do.goOnline(); }
</script>

<div class="connect-page">
  <div class="intro">
    <h1>Connect</h1>
  </div>

  <section class="session-card" aria-label="Connection status" style:--status-color={session.color}>
    <span class="session-symbol" aria-hidden="true">
      <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7"><path d="M4 7h16v13H4zM8 11v5m4-5v5m4-5v5M8 3h8" /></svg>
    </span>
    <div class="session-copy" role="status" aria-live="polite">
      <strong>{session.label}</strong>
      {#if session.connected}<p>{session.detail}</p>{:else if session.demo}<p>Simulator</p>{/if}
    </div>
    {#if session.connected}
      <div class="session-actions"><button class="primary" disabled={busy} onclick={() => ui.do.openPage('mixer')}>Open mixer</button><button disabled={flow.cancelling} onclick={cancel}>Disconnect</button></div>
    {:else if offline}
      <button class="primary" onclick={() => ui.do.goOnline()}>Go online</button>
    {:else if session.starting && !pending}
      <button onclick={cancel} disabled={flow.cancelling}>Cancel</button>
    {:else if !session.ready}
      <button onclick={retryService} disabled={!!pending || flow.cancelling || flow.cleanupRequired}>Retry service</button>
    {/if}
  </section>

  {#if !offline}
    {#if !session.ready && live.phase === 'error'}
      <p class="feedback error" role="alert">{live.message}</p>
    {/if}
    {#if flow.cancelling}
      <div class="feedback attempt" role="status"><div><strong>Cancelling…</strong></div></div>
    {:else if pending}
      <div class="feedback attempt" role="status">
        <div><strong>{pending.phase === 'service' ? 'Starting service…' : pending.phase === 'failed' ? 'Connection failed' : pending.phase === 'slow' ? 'No response yet' : `Connecting to ${pending.label}…`}</strong></div>
        <div class="session-actions">{#if ['slow', 'failed'].includes(pending.phase)}<button onclick={() => ui.do.retryConnection()}>Retry</button>{/if}<button onclick={cancel}>Cancel</button></div>
      </div>
    {/if}
    {#if flow.cleanupRequired && !pending && !flow.cancelling}<button onclick={cancel}>Retry cancellation</button>{/if}
    {#if error}<p class="feedback error" role="alert">{error}</p>{/if}

    {#if recents.length}
      <section class="option recent-section" aria-labelledby={`${fieldId}-recents`}>
        <h2 id={`${fieldId}-recents`}>Recent mixers</h2>
        <div class="recent-cards">{#each recents as desk (desk.host)}
          <button class="recent-card" onclick={() => manual(desk.host)} disabled={busy} aria-label={`Connect to ${desk.name || desk.host}`}>
            <strong>{desk.name || desk.host}</strong>
            <span>{[desk.model, desk.name ? desk.host : '', desk.resolvedIp && desk.resolvedIp !== desk.host ? desk.resolvedIp : ''].filter(Boolean).join(' · ')}</span>
            <small>{desk.connectedAt ? 'Previously used' : 'Unverified address'}</small>
          </button>
        {/each}</div>
      </section>
    {/if}
    <div class="connection-options">
      <section class="option manual" aria-labelledby={`${fieldId}-manual`}>
        <div class="option-heading"><div><h2 id={`${fieldId}-manual`}>Connect IP address</h2></div></div>
        <form onsubmit={e => { e.preventDefault(); manual(); }}>
          <label class="sr-only" for={fieldId}>IP address</label>
          <p id={`${fieldId}-example`} class="example">Example: 192.168.50.123</p>
          <input id={fieldId} bind:value={ui.connectionAddress} aria-describedby={`${fieldId}-example`} autocapitalize="off" autocomplete="off" autocorrect="off" spellcheck={false} inputmode="decimal" disabled={busy} />
          <button type="submit" disabled={busy || !ui.connectionAddress.trim()}>Connect</button>
        </form>
      </section>
      <section class="option discovery" aria-labelledby={`${fieldId}-discovery`}>
        <div class="option-heading"><div><h2 id={`${fieldId}-discovery`}>Search network</h2></div></div>
        <button class="scan" onclick={scan} disabled={!session.ready || busy} aria-busy={vm.scanBusy}>
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true"><path d="M3 8a14 14 0 0 1 18 0M6 12a9 9 0 0 1 12 0M9 16a4 4 0 0 1 6 0"/><circle cx="12" cy="20" r="1" /></svg>
          {vm.scanBusy ? 'Searching…' : 'Find mixers'}
        </button>
        {#if flow.scanPending}<button onclick={cancel} disabled={flow.cancelling}>Cancel search</button>{/if}
        {#if !session.ready || vm.scanBusy || (live.engine?.scannedAt ?? 0) > 0}<div class="scan-meta" role="status">{!session.ready ? (session.starting ? 'Starting service…' : 'Service unavailable') : vm.lastScan}</div>{/if}
        <div class="results" aria-busy={vm.scanBusy} aria-live="polite">
          {#each vm.devices as desk (desk.key)}
            <article class="device" class:active={desk.status === 'Connected'}>
              <div class="device-copy"><strong>{desk.name || 'Mixer'}</strong><p>{desk.sub}</p><span>{desk.status}</span></div>
              {#if desk.canConnect}<button onclick={() => connect(desk.key)} disabled={!session.ready || busy}>Connect<span class="sr-only"> to {desk.name}</span></button>{/if}
            </article>
          {/each}
          {#if vm.scanBusy}<div class="empty"><span class="pulse" aria-hidden="true"></span><p>Searching…</p></div>
          {:else if vm.scanError}<div class="empty error" role="alert"><p>{vm.scanError}</p><button onclick={scan} disabled={!session.ready || busy}>Retry scan</button></div>
          {:else if vm.emptyText && (live.engine?.scannedAt ?? 0) > 0}<div class="empty"><p>No mixers found.</p></div>{/if}
        </div>
      </section>

    </div>
  {/if}

  <section class="offline-option" aria-label="Offline preview">
    <button disabled={flow.cancelling} onclick={() => offline ? ui.do.openPage('mixer') : ui.do.goOffline()}>{offline ? 'Open offline mixer' : 'Work offline'} <span aria-hidden="true">→</span></button>
  </section>
</div>

<style>
  .connect-page { box-sizing:border-box; width:100%; max-width:980px; margin:0 auto; padding:18px 8px 28px; display:flex; flex-direction:column; gap:18px; color:var(--u4-text-1); font-family:inherit; }
  .connect-page *, .connect-page *::before, .connect-page *::after { box-sizing:border-box; }
  .intro { padding:2px 2px 4px; }
  h1 { margin:8px 0; font-size:clamp(28px,4vw,38px); letter-spacing:-.045em; line-height:1.1; font-weight:650; }
  h2 { margin:0; font-size:16px; font-weight:650; letter-spacing:-.02em; }
  p { margin:0; color:var(--u4-text-5); font-size:13px; line-height:1.55; overflow-wrap:anywhere; }
  .session-card { display:flex; align-items:center; gap:14px; padding:18px; background:var(--u4-panel-bg); border:1px solid var(--u4-line-panel); border-radius:12px; }
  .session-symbol { display:grid; place-items:center; width:44px; height:44px; flex:none; color:var(--status-color); background:var(--u4-well-bg); border-radius:10px; }
  .session-copy { flex:1; min-width:0; }.session-copy strong { font-size:16px; overflow-wrap:anywhere; }.session-copy p { margin-top:4px; }
  .session-actions { display:flex; flex-wrap:wrap; gap:8px; }
  .connection-options { display:grid; grid-template-columns:minmax(0,1.15fr) minmax(0,1fr); gap:16px; align-items:start; }
  .option { padding:22px; border:1px solid var(--u4-line-panel); border-radius:12px; background:var(--u4-panel-bg); display:flex; flex-direction:column; gap:16px; min-width:0; }
  .option-heading { display:flex; gap:12px; align-items:flex-start; }
  button { min-height:48px; min-width:44px; padding:10px 16px; border:1px solid var(--u4-line-ctl); border-radius:8px; background:var(--u4-btn-bg); color:var(--u4-text-1); font:inherit; font-size:13px; font-weight:600; cursor:pointer; line-height:1.3; overflow-wrap:anywhere; }
  button:hover:enabled { filter:brightness(1.1); }button:disabled { opacity:.42; cursor:default; }
  button.primary { background:var(--u4-green); color:#10240f; border-color:transparent; }
  button:focus-visible, input:focus-visible { outline:2px solid var(--u4-green); outline-offset:3px; }
  .scan { width:100%; display:flex; gap:10px; justify-content:center; align-items:center; font-size:14px; }
  .scan-meta { margin-top:-7px; text-align:center; font-size:11px; color:var(--u4-text-6); }
  .results { min-width:0; display:flex; flex-direction:column; gap:10px; }
  .empty { min-height:100px; padding:16px; border:1px dashed var(--u4-line-ctl); border-radius:8px; display:flex; align-items:center; justify-content:center; gap:12px; text-align:center; }.empty p { font-size:12px; max-width:280px; }
  .device { display:flex; align-items:center; gap:12px; padding:14px; background:var(--u4-well-bg); border:1px solid var(--u4-line-ctl); border-radius:8px; }.device.active { border-color:var(--u4-green); }.device-copy { flex:1; min-width:0; }.device strong { font-size:15px; overflow-wrap:anywhere; }.device p { font-size:11px; margin:4px 0; }.device span { font-size:11px; color:var(--u4-text-5); }.device.active span { color:var(--u4-green); }
  form { display:flex; flex-direction:column; gap:8px; }label { font-size:12px; font-weight:650; }.example { font-size:11px; }input { min-width:0; width:100%; height:50px; padding:0 12px; background:var(--u4-well-bg); border:1px solid var(--u4-line-ctl); border-radius:8px; color:var(--u4-text-1); font:inherit; font-size:17px; letter-spacing:.02em; }form button { margin-top:4px; }
  .recent-cards { display:grid; grid-template-columns:repeat(auto-fit,minmax(min(100%,240px),1fr)); gap:10px; }
  .recent-card { min-height:94px; width:100%; text-align:left; display:flex; flex-direction:column; justify-content:center; gap:5px; padding:16px; }
  .recent-card strong { font-size:17px; font-weight:600; }.recent-card span,.recent-card small { color:var(--u4-text-5); font-weight:400; font-size:12px; }
  .feedback { padding:14px 16px; border:1px solid var(--u4-line-ctl); border-radius:8px; background:var(--u4-well-bg); }.error { color:#ff9f91; }.error p { color:inherit; }.attempt { display:flex; gap:12px; align-items:center; justify-content:space-between; }.attempt strong { font-size:14px; }
  .offline-option { padding:4px 2px; display:flex; align-items:center; gap:24px; justify-content:space-between; }.offline-option button { flex:none; background:transparent; }
  .pulse { flex:none; width:12px; height:12px; background:var(--u4-green); border-radius:50%; animation:pulse 1s ease-in-out infinite alternate; }@keyframes pulse { to { opacity:.25; } }@media(prefers-reduced-motion:reduce) { .pulse { animation:none; } }
  .sr-only { position:absolute; width:1px; height:1px; padding:0; margin:-1px; overflow:hidden; clip:rect(0,0,0,0); white-space:nowrap; }
  @media(max-width:650px) { .connect-page { padding:8px 0 20px; gap:14px; }.connection-options { grid-template-columns:minmax(0,1fr); gap:14px; }.option { padding:18px; gap:14px; }.session-card { padding:14px; gap:10px; flex-wrap:wrap; }.session-symbol { width:36px; height:36px; }.session-copy { flex-basis:calc(100% - 50px); }.session-card>button, .session-actions { width:100%; }.session-actions button { flex:1; }.offline-option { align-items:flex-start; flex-direction:column; gap:12px; }.offline-option button { width:100%; } }
</style>

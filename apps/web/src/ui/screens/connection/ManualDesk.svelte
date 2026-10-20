<!-- Legacy layout module: uses the same attempt lifecycle as Connect. -->
<script lang="ts">
  import { ui } from '../../state/uiStore.svelte.ts';
  import { live } from '../../state/live.svelte.ts';
  let { onEvent }: { onEvent?: (name: string, payload?: unknown) => void } = $props();
  const fieldId = $props.id();
  const recents = $derived.by(() => { void live.version; return live.recentDesks(); });
  const flow = $derived(ui.connectionFlow);
  const busy = $derived(!!flow.attempt || flow.cancelling || flow.cleanupRequired);
  function add(raw: string) { ui.do.probeDesk(raw); }
</script>

<section class="manual-desk" aria-labelledby={`${fieldId}-title`}>
  <h2 id={`${fieldId}-title`}>Connect IP address</h2>
  <form onsubmit={e => { e.preventDefault(); add(ui.connectionAddress); }}>
    <label class="sr-only" for={fieldId}>IP address</label>
    <p id={`${fieldId}-example`} class="example">Example: 192.168.50.123</p>
    <div class="address-row">
      <input
        id={fieldId}
        bind:value={ui.connectionAddress}
        aria-describedby={`${fieldId}-example`}
        autocapitalize="off"
        autocomplete="off"
        autocorrect="off"
        spellcheck={false}
        inputmode="decimal"
      />
      <button type="submit" disabled={busy || !ui.connectionAddress.trim()}>Connect</button>
    </div>
  </form>
  {#if recents.length > 0}
    <div class="recents">
      <p>Recent addresses</p>
      <div class="recent-addresses">
        {#each recents as r (r.host)}
          <button onclick={() => add(r.host)} disabled={busy} aria-label="Connect to {r.host}">{r.name || r.host} · {r.connectedAt ? 'Previously used' : 'Previously entered'}</button>
        {/each}
      </div>
    </div>
  {/if}
  {#if flow.attempt || flow.error || flow.cancelling}<p class="status" role="status">{flow.attempt?.error || flow.error || (flow.cancelling ? 'Cancelling…' : flow.attempt ? 'Connecting…' : '')}
    {#if flow.attempt}<button onclick={() => ui.do.cancelConnection()}>Cancel</button>{/if}</p>{/if}
</section>

<style>
  .sr-only { position:absolute; width:1px; height:1px; padding:0; margin:-1px; overflow:hidden; clip:rect(0,0,0,0); white-space:nowrap; }
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

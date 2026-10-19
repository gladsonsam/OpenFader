<!-- L1 module `relay-card`: the real relay connection card.
     Presentational: live relay state in via layout `bind` (host/phase/
     message/latency/strips/suggestion), `connect` (payload carries the typed
     host), `use-page-host` (one-tap LAN fix: point at the machine serving
     this page) and `disconnect` events out. The host persists (localStorage)
     and `?server=` overrides it; failures surface as the message line, and
     when the stored host is unreachable from this device the suggestion
     button names what to use instead (never 127.0.0.1 for a remote client). -->
<script lang="ts">
  import type { ModuleContext } from '../../modules/types.ts';

  let {
    props = {},
    ctx: _ctx,
    nodeId: _nodeId,
    onEvent,
  }: {
    props: {
      host?: string;
      phase?: string;
      message?: string;
      latencyMs?: number | null;
      strips?: number;
      suggestion?: string | null;
    };
    ctx: ModuleContext;
    nodeId: string;
    onEvent?: (name: string, payload?: unknown) => void;
  } = $props();

  const host = $derived(props.host ?? '127.0.0.1:8080');
  const phase = $derived(props.phase ?? 'idle');
  const message = $derived(props.message ?? 'Not connected');
  const online = $derived(phase === 'online');
  const busy = $derived(phase === 'connecting');
  const suggestion = $derived(props.suggestion ?? null);
  const showFix = $derived(phase === 'error' && !!suggestion);

  let draft = $state('');
  let input: HTMLInputElement | null = $state(null);
  $effect(() => {
    // Adopt the bound host (stored value, ?server=) unless being edited.
    const h = host;
    if (document.activeElement !== input && draft !== h) draft = h;
  });

  const dot = $derived(
    online
      ? 'var(--u4-conn-dot)'
      : busy
        ? 'var(--u4-amber)'
        : phase === 'error'
          ? 'var(--u4-red)'
          : 'var(--u4-text-7)',
  );
  const statusText = $derived(
    online
      ? `Live · ${props.strips ?? 0} strips` +
        (props.latencyMs != null ? ` · handshake ${Math.round(props.latencyMs)} ms` : '')
      : busy
        ? 'Connecting …'
        : phase === 'error'
          ? 'Connection failed'
          : 'Relay',
  );
</script>

<div
  style="box-sizing:content-box;display:flex;flex-direction:column;gap:10px;padding:12px 14px;border-radius:3px;border:1px solid var(--u4-line-panel);background:var(--u4-well-bg)"
>
  <div style="box-sizing:content-box;display:flex;align-items:center;gap:10px">
    <span
      style="width:10px;height:10px;flex:none;border-radius:50%;background:{dot};box-shadow:0 0 8px {dot}"
    ></span>
    <div style="box-sizing:content-box;flex:1;min-width:0">
      <div style="box-sizing:content-box;font-size:16px;font-weight:600">{statusText}</div>
      <div
        style="box-sizing:content-box;font-size:12px;color:var(--u4-text-7);white-space:nowrap;overflow:hidden;text-overflow:ellipsis"
      >{message}</div>
    </div>
  </div>
  <div style="box-sizing:content-box;display:flex;gap:8px;flex-wrap:wrap">
    <input
      bind:this={input}
      bind:value={draft}
      aria-label="Relay host"
      placeholder="192.168.1.120:8080"
      autocapitalize="off"
      autocomplete="off"
      autocorrect="off"
      spellcheck={false}
      inputmode="url"
      style="box-sizing:border-box;flex:1 1 160px;min-width:0;height:44px;padding:0 12px;border-radius:3px;border:1px solid var(--u4-line-ctl);background:var(--u4-panel-bg);color:var(--u4-text-1);font-size:16px;font-family:inherit"
    />
    {#if online}
      <button
        onclick={() => onEvent?.('disconnect')}
        aria-label="Disconnect relay"
        style="box-sizing:border-box;height:44px;min-width:104px;padding:0 14px;border-radius:3px;border:1px solid var(--u4-line-ctl);background:var(--u4-btn-bg);color:var(--u4-text-1);font-size:14px;cursor:pointer;font-variant-numeric:normal"
        >Disconnect</button
      >
    {:else}
      <button
        onclick={() => onEvent?.('connect', { host: draft })}
        disabled={busy}
        aria-label="Connect to relay"
        style="box-sizing:border-box;height:44px;min-width:104px;padding:0 14px;border-radius:3px;border:1px solid var(--u4-green);background:{busy ? 'var(--u4-btn-bg)' : 'var(--u4-green)'};color:{busy ? 'var(--u4-text-7)' : 'var(--u4-green-ink)'};font-size:14px;font-weight:600;cursor:pointer;font-variant-numeric:normal"
        >{busy ? 'Connecting…' : 'Connect'}</button
      >
    {/if}
  </div>
  {#if showFix}
    <button
      onclick={() => onEvent?.('use-page-host')}
      aria-label="Use this page's host for the relay"
      style="box-sizing:border-box;min-height:44px;padding:8px 14px;border-radius:3px;border:1px dashed var(--u4-green);background:transparent;color:var(--u4-text-1);font-size:14px;cursor:pointer;text-align:left;font-variant-numeric:normal"
      >On another device? This page is served from this network — use <strong>{suggestion}</strong> instead</button
    >
  {/if}
</div>

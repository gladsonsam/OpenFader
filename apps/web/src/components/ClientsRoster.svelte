<script lang="ts">
  /**
   * ClientsRoster — who else is on the relay.
   *
   * The relay `clients` frame carries `{count, writers}` only (ids, no
   * labels — verified against crates/serve `clients_message`); `writers`
   * are ids with a mutation in the last 10 s. `selfId` is learned
   * client-side from the first `by` echo of our own write (null until we
   * write anything). Rendered only when the transport feeds it (WS);
   * Tauri has no presence feed and hides this panel.
   */
  interface Props {
    count: number;
    writers: string[];
    selfId: string | null;
  }
  const { count, writers, selfId }: Props = $props();

  const others = $derived(writers.filter((w) => w !== selfId));
</script>

<div id="roster" role="status" aria-label={`connected clients: ${count}`}>
  <span class="lbl">clients</span>
  <span class="n" title={`${count} client${count === 1 ? '' : 's'} connected`}>{count}</span>
  {#if selfId && writers.includes(selfId)}
    <span class="who mine" title="this device — active in the last 10 s">● {selfId} · you</span>
  {/if}
  {#each others as w (w)}
    <span class="who" title={`${w} — active in the last 10 s`}>● {w}</span>
  {/each}
  {#if !writers.length}
    <span class="idle">all idle</span>
  {/if}
</div>

<style>
  /* Mirrors the locks strip (#locks in app.css): same row, same chips. */
  #roster {
    display: flex; gap: 8px; align-items: center; padding: 6px 10px; flex-wrap: wrap;
    border-bottom: 1px solid var(--line); background: var(--panel); font-size: 13px;
  }
  #roster .lbl { color: var(--dim); }
  #roster .n {
    font-weight: 800; border: 1px solid var(--line); border-radius: 8px; padding: 4px 10px;
  }
  #roster .who {
    display: inline-flex; gap: 4px; align-items: center;
    border: 1px solid var(--yel); border-radius: 8px; padding: 4px 10px;
    min-height: 0; font-size: 13px; color: var(--yel); cursor: default;
  }
  #roster .who.mine { border-color: var(--grn); color: #7dffa4; }
  #roster .idle { color: var(--dim); }
</style>

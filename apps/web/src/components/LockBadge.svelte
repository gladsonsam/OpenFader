<script lang="ts">
  // Strip lock badge: lock state affordance per strip.
  // Unlocked: faint "lock" affordance, tap claims the strip.
  // Held by me: green "mine", tap releases.
  // Held by another client: red "held by <id>", tap retries (server denies,
  // and the denial surfaces the holder id via the status line).

  interface Props {
    ch: number;
    holder: string | null;
    mine: boolean;
    onToggle: () => void;
  }

  let { ch, holder, mine, onToggle }: Props = $props();

  const label = $derived(
    holder === null ? 'lock' : mine ? 'mine' : `held`,
  );
  const title = $derived(
    holder === null
      ? `lock ch${ch < 0 ? 'M' : ch + 1} (claim while editing)`
      : mine
        ? `you hold ch${ch + 1} — tap to release`
        : `held by ${holder} — tap to try anyway`,
  );
</script>

<button
  class="lockbadge"
  class:mine={mine}
  class:held={holder !== null && !mine}
  aria-pressed={holder !== null}
  aria-label={holder === null
    ? `lock channel ${ch + 1}`
    : mine
      ? `unlock channel ${ch + 1} (you hold it)`
      : `channel ${ch + 1} held by ${holder}`}
  {title}
  onclick={(e) => { e.stopPropagation(); onToggle(); }}
>
  {#if holder === null}
    <span aria-hidden="true">○</span> {label}
  {:else if mine}
    <span aria-hidden="true">●</span> {label}
  {:else}
    <span aria-hidden="true">●</span> {label} <small>{holder}</small>
  {/if}
</button>

<style>
  .lockbadge {
    width: 100%;
    min-height: 32px;
    min-width: 0;
    padding: 0 4px;
    font-size: 11px;
    font-weight: 700;
    border-radius: 6px;
    background: none;
    border: 1px solid transparent;
    color: var(--dim);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .lockbadge small {
    font-weight: 400;
    opacity: 0.85;
  }
  .lockbadge.mine {
    border-color: var(--grn);
    color: var(--grn);
    background: rgba(70, 196, 106, 0.08);
  }
  .lockbadge.held {
    border-color: var(--red);
    color: #ff8a8e;
    background: rgba(229, 72, 77, 0.08);
  }
</style>

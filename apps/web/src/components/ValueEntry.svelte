<script lang="ts">
  // Tap-a-value exact entry (safety pattern): type the fader/send level
  // instead of dragging it. Value is 0..1; `format` renders the unit.
  interface Props {
    title: string;
    sub: string;
    value: number;
    format: (v: number) => string;
    onApply: (v: number) => void;
    onCancel: () => void;
  }

  let { title, sub, value, format, onApply, onCancel }: Props = $props();

  let pct: number = $state(0);

  $effect(() => {
    // Re-seed whenever a new exact-entry session opens (parent remounts us).
    pct = Math.round(value * 100);
  });

  function apply(): void {
    onApply(Math.min(1, Math.max(0, pct / 100)));
  }

  function onWinKey(e: KeyboardEvent): void {
    if (e.key === 'Escape') onCancel();
  }
</script>

<svelte:window onkeydown={onWinKey} />

<div class="vmodal" role="dialog" aria-modal="true" aria-label="exact value entry">
  <button class="vback" onclick={onCancel} aria-label="cancel"></button>
  <form
    class="vcard"
    onsubmit={(e) => { e.preventDefault(); apply(); }}
  >
    <h3>{title}</h3>
    <div class="vsub">{sub} — currently <b>{format(value)}</b></div>
    <input
      type="range"
      min="0"
      max="100"
      step="1"
      bind:value={pct}
      aria-label="level percent"
    />
    <div class="vrow">
      <input
        type="number"
        min="0"
        max="100"
        step="1"
        bind:value={pct}
        aria-label="level percent number"
      />
      <span class="vunit">% = <b>{format(pct / 100)}</b></span>
    </div>
    <div class="vquick" aria-label="quick values">
      {#each [0, 25, 50, 75, 100] as q (q)}
        <button type="button" onclick={() => { pct = q; }}>{q}</button>
      {/each}
    </div>
    <div class="vrow">
      <button type="submit" class="big">set</button>
      <button type="button" onclick={onCancel}>cancel</button>
    </div>
  </form>
</div>

<script lang="ts">
  import { onMount } from 'svelte';
  import type { Snippet } from 'svelte';

  interface Props {
    paneA: Snippet;
    paneB: Snippet;
  }

  let { paneA, paneB }: Props = $props();

  // Divider position: % of width given to pane A. Draggable; persists
  // for the session only (layout pref, not a mix pref).
  let pos = $state(50);
  let narrow = $state(false);
  let root: HTMLDivElement | null = $state(null);
  let dragging = $state(false);

  onMount(() => {
    const mq = window.matchMedia('(max-width: 1099px)');
    const sync = (): void => {
      narrow = mq.matches;
    };
    sync();
    mq.addEventListener('change', sync);
    return () => mq.removeEventListener('change', sync);
  });

  function onDown(e: PointerEvent): void {
    if (narrow) return;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    dragging = true;
  }

  function onMove(e: PointerEvent): void {
    if (!dragging || !root || narrow) return;
    const r = root.getBoundingClientRect();
    if (r.width <= 0) return;
    pos = Math.min(80, Math.max(20, ((e.clientX - r.left) / r.width) * 100));
  }

  function onUp(): void {
    dragging = false;
  }

  function onKeys(e: KeyboardEvent): void {
    if (e.key === 'ArrowLeft' || e.key === 'ArrowRight') {
      pos = Math.min(80, Math.max(20, pos + (e.key === 'ArrowRight' ? 2 : -2)));
      e.preventDefault();
    } else if (e.key === 'Home') {
      pos = 50;
      e.preventDefault();
    }
  }
</script>

<div
  class="split"
  class:narrow
  bind:this={root}
  style={`--split: ${pos}%`}
  aria-label={narrow ? 'dual mixer (stacked: second pane hidden on narrow screens)' : 'dual mixer'}
>
  <section class="pane pane-a" aria-label="mixer pane A">
    {@render paneA()}
  </section>
  {#if !narrow}
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <div
      class="divider"
      class:grab={dragging}
      role="separator"
      aria-orientation="vertical"
      aria-label="pane divider (drag or arrow keys, Home resets)"
      aria-valuemin={20}
      aria-valuemax={80}
      aria-valuenow={Math.round(pos)}
      tabindex={0}
      onpointerdown={onDown}
      onpointermove={onMove}
      onpointerup={onUp}
      onpointercancel={onUp}
      onkeydown={onKeys}
    ></div>
    <section class="pane pane-b" aria-label="mixer pane B">
      {@render paneB()}
    </section>
  {/if}
</div>

<style>
  .split { display: flex; gap: 0; align-items: stretch; padding: 10px; }
  .pane { min-width: 0; display: flex; }
  .pane-a { flex: 0 0 var(--split); }
  .pane-b { flex: 1; }
  .pane :global(.mpane) { width: 100%; }
  .divider {
    flex: 0 0 14px; cursor: col-resize; border-radius: 7px; margin: 0 6px;
    background: var(--line); border: 1px solid var(--line);
    touch-action: none; min-height: 200px;
  }
  .divider:hover, .divider.grab { background: var(--acc); border-color: var(--acc); }
</style>

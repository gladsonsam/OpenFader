<!-- Full-viewport column frame, exactly like the template's root div:
     height 100vh, min-height 600px, flex column, page bg, root text colour,
     Geist, tabular-nums, hidden overflow. Screens render via children. -->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import { ui } from './state/uiStore.svelte.ts';
  import { PRESETS, applyTheme, clearTheme } from './modules/theme.ts';

  let { children }: { children?: Snippet } = $props();

  // Theme presets are token overrides applied as inline variables on the
  // frame (design-dark = no overrides: tokens.css is the truth). Switching
  // clears the previous set first so maps never leak into each other.
  let frame: HTMLElement | null = $state(null);
  let applied: string[] = [];
  $effect(() => {
    const name = ui.state.theme;
    if (frame) {
      clearTheme(applied, frame);
      applied = applyTheme({ ...PRESETS[name] }, frame);
      frame.dataset.theme = name;
    }
  });
</script>

<div
  bind:this={frame}
  class="u4-frame"
  data-canvas-width="1280"
  data-canvas-height="720"
  data-document-role="page"
>
  {@render children?.()}
</div>

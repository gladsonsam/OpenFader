<!-- Full-viewport column frame, exactly like the template's root div:
     height 100vh, min-height 600px, flex column, page bg, root text colour,
     Geist, tabular-nums, hidden overflow. Screens render via children. -->
<script lang="ts">
  import type { Snippet } from 'svelte';

  let { children }: { children?: Snippet } = $props();
</script>

<div class="u4-frame" data-canvas-width="1280" data-canvas-height="720" data-document-role="page">
  {@render children?.()}
</div>

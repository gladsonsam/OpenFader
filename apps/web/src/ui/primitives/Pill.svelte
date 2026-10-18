<!-- Small label chip (Pre/Post send tap, On/Bypassed, Gate/Comp In, scene
     Current tag). Template: Pre selected = var(--u4-text-1) bg / #0b0f14 text;
     Post = var(--u4-btn-bg) / var(--u4-text-4); On = #5cdb5c / #10240f; Bypassed text #f0b44a;
     font 11-13px w600 for status words. Non-interactive by default. -->
<script lang="ts">
  import type { Snippet } from 'svelte';

  export type PillTone = 'light' | 'dark' | 'green' | 'amber' | 'red' | 'ghost';

  let {
    tone = 'dark',
    children,
  }: {
    tone?: PillTone;
    children: Snippet;
  } = $props();

  const style = $derived.by(() => {
    switch (tone) {
      case 'light':
        return 'background:var(--u4-seg-on-bg);color:var(--u4-seg-on-fg)';
      case 'green':
        return 'background:var(--u4-green);color:var(--u4-green-ink)';
      case 'amber':
        return 'background:transparent;color:var(--u4-amber)';
      case 'red':
        return 'background:transparent;color:var(--u4-red)';
      case 'ghost':
        return 'background:transparent;color:var(--u4-text-5)';
      default:
        return 'background:var(--u4-seg-off-bg);color:var(--u4-seg-off-fg)';
    }
  });
</script>

<span style="display:inline-block;font-size:12px;font-weight:600;white-space:nowrap;{style}">
  {@render children()}
</span>

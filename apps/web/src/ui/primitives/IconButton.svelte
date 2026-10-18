<!-- 46x46 header-style icon button (FX/Routing/Connect/Settings).
     Template: width/height 46px, radius 3px, border #2a323f inactive /
     #5a6577 active, bg #0d1219 / #232b37, colour #dfe6ef, optional 8px
     status dot with `box-shadow: 0 0 0 2px #0d1219` ring (Connect button). -->
<script lang="ts">
  import type { Snippet } from 'svelte';

  let {
    label,
    active = false,
    dot = null,
    onclick = null,
    children,
  }: {
    label: string;
    active?: boolean;
    /** status dot colour (e.g. connection dot); null hides it */
    dot?: string | null;
    onclick?: ((e: MouseEvent) => void) | null;
    children: Snippet;
  } = $props();
</script>

<button
  aria-label={label}
  aria-current={active ? 'page' : 'false'}
  onclick={onclick}
  style="width:var(--u4-iconbtn);height:var(--u4-iconbtn);position:relative;border-radius:var(--u4-r-3);border:1px solid {active
    ? 'var(--u4-tile-border-on)'
    : 'var(--u4-tile-border)'};background:{active
    ? 'var(--u4-tile-bg-on)'
    : 'var(--u4-tile-bg)'};color:var(--u4-text-3);cursor:pointer;display:flex;align-items:center;justify-content:center;padding:0"
>
  {@render children()}
  {#if dot !== null}
    <span
      style="position:absolute;right:6px;top:6px;width:8px;height:8px;border-radius:50%;background:{dot};box-shadow:0 0 0 2px #0d1219"
    ></span>
  {/if}
</button>

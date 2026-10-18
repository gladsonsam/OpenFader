<!-- Titled card (Display/Faders/Safety/About in Settings, Channels/Sources in
     Routing, Preamp/Source/Delay in Input, FX detail, Connection cards).
     Template: radius 3px, border 1px #1f2836, bg #0e1520; section title
     padding 12px 14px 4px, font 12px colour #8f9bb0. -->
<script lang="ts">
  import type { Snippet } from 'svelte';

  let {
    title,
    labelledBy = null,
    padding = 0,
    children,
  }: {
    title: string;
    /** when the panel body supplies its own labelled section, pass its id */
    labelledBy?: string | null;
    padding?: number;
    children: Snippet;
  } = $props();
</script>

<section
  aria-label={labelledBy === null ? title : undefined}
  aria-labelledby={labelledBy ?? undefined}
  style="display:flex;flex-direction:column;border-radius:var(--u4-panel-radius);border:1px solid var(--u4-panel-border);background:var(--u4-panel-bg){padding !==
  0
    ? `;padding:${padding}px`
    : ''}"
>
  {#if labelledBy === null}
    <div style="padding:12px 14px 4px;font-size:12px;color:var(--u4-text-6)">{title}</div>
  {/if}
  {@render children()}
</section>

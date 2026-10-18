<!-- Segmented option group (Settings Off/On rows, 8/12/16 strips, route
     bank chips, EQ type chips, send-target 1-16 pads, Confirm/Instant).
     Template: selected = light bg (#e8edf5 / #eef2f7) with dark text #0b0f14,
     unselected dark (#2b3038, settings #1b222d) with #c9d1dc text;
     Settings rows: h40 min-w 52 radius 3, selected border #e8edf5,
     unselected #343d4b; route chips h36 border #3a414c. -->
<script lang="ts">
  export interface SegmentOption {
    value: string;
    label: string;
    selected: boolean;
  }

  let {
    groupLabel,
    options,
    height = 40,
    minWidth = 52,
    radius = 3,
    darkBg = null,
    darkBorder = null,
    onpick,
  }: {
    groupLabel: string;
    options: SegmentOption[];
    height?: number;
    minWidth?: number;
    radius?: number;
    /** override for unselected bg (settings rows use #1b222d) */
    darkBg?: string | null;
    /** override for unselected border (settings rows use #343d4b) */
    darkBorder?: string | null;
    onpick: (value: string) => void;
  } = $props();

  const offBg = $derived(darkBg ?? 'var(--u4-seg-off-bg)');
  const offBorder = $derived(darkBorder ?? 'var(--u4-line-ctl)');
</script>

<div role="group" aria-label={groupLabel} style="display:flex;flex-wrap:wrap;gap:4px">
  {#each options as o (o.value)}
    <button
      aria-pressed={o.selected ? 'true' : 'false'}
      onclick={() => onpick(o.value)}
      style="height:{height}px;min-width:{minWidth}px;padding:0 12px;border-radius:{radius}px;border:1px solid {o.selected
        ? 'var(--u4-seg-on-bg)'
        : offBorder};background:{o.selected
        ? 'var(--u4-seg-on-bg)'
        : offBg};color:{o.selected
        ? 'var(--u4-seg-on-fg)'
        : 'var(--u4-seg-off-fg)'};font-size:14px;cursor:pointer"
    >
      {o.label}
    </button>
  {/each}
</div>

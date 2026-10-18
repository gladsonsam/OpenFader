<!-- Strip name bar, pixel-matched to the design's select button.
     Strip/master: 52px, name 16px w600 + src 12px at 80% opacity.
     Channel plate: 54px centred name 17px w600 (src is a separate line). -->
<script lang="ts">
  let {
    name,
    sub = '',
    color,
    text,
    variant = 'strip',
    selectAria = null,
    onselect = null,
  }: {
    name: string;
    sub?: string;
    color: string;
    text: string;
    variant?: 'strip' | 'plate';
    /** When set, renders as a button with this aria-label (design: 'Select Ch 1, source Lcl 01'). */
    selectAria?: string | null;
    onselect?: (() => void) | null;
  } = $props();

  const h = variant === 'plate' ? 54 : 52;
  const nameSize = variant === 'plate' ? 17 : 16;
</script>

{#if selectAria}
  <button
    aria-label={selectAria}
    class="tpress"
    onclick={onselect
      ? (e) => {
          // Don't bubble to a selectable ancestor (mixer strip): the name
          // tap opens the editor, and the strip select firing after it
          // would close the editor again immediately.
          e.stopPropagation();
          onselect();
        }
      : undefined}
    style="height:{h}px;flex:none;display:flex;flex-direction:column;align-items:center;justify-content:center;gap:1px;border-radius:var(--u4-r-3);border:1px solid rgba(0,0,0,.35);background:{color};color:{text};cursor:pointer;padding:0 3px;min-width:0;overflow:hidden;width:100%"
  >
    <span
      style="display:block;max-width:100%;font-size:{nameSize}px;font-weight:600;line-height:1.1;white-space:nowrap;overflow:hidden;text-overflow:ellipsis"
      >{name}</span
    >
    {#if variant === 'strip'}
      <span
        style="display:block;max-width:100%;font-size:12px;line-height:1.1;opacity:.8;white-space:nowrap;overflow:hidden;text-overflow:ellipsis"
        >{sub}</span
      >
    {/if}
  </button>
{:else}
  <div
    style="height:{h}px;flex:none;display:flex;align-items:center;justify-content:center;border-radius:var(--u4-r-3);background:{color};color:{text};font-size:{nameSize}px;font-weight:600;overflow:hidden"
  >
    {name}
  </div>
{/if}

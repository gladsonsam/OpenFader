<!-- Strip name bar, pixel-matched to the design's select button.
     Strip/master: 76px, name 16px w600 centred with the 44px channel icon
     stacked above (taller chips so icon + name never clip).
     sub-line is intentionally gone: the routing page owns patching now,
     and screen readers still get the source via selectAria.)
     Channel plate: 54px centred name 17px w600. -->
<script lang="ts">
  import { chanIcons, type ChanIconName } from '../../icons.ts';

  let {
    name,
    color,
    text,
    icon = null,
    variant = 'strip',
    selectAria = null,
    onselect = null,
  }: {
    name: string;
    sub?: string;
    color: string;
    text: string;
    /** Channel icon name (null = none). Rendered left of the name. */
    icon?: string | null;
    variant?: 'strip' | 'plate';
    /** When set, renders as a button with this aria-label (design: 'Select Ch 1, source Lcl 01'). */
    selectAria?: string | null;
    onselect?: (() => void) | null;
  } = $props();

  /** Guarded lookup: fixtures default to null; the store only writes known names. */
  const iconDef = $derived(icon != null && icon in chanIcons ? chanIcons[icon as ChanIconName] : null);
  const h = $derived(variant === 'plate' ? (iconDef ? 76 : 54) : 76);
  const nameSize = $derived(variant === 'plate' ? 17 : 16);
  const iconSize = 44;
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
    style="height:{h}px;flex:none;display:flex;flex-direction:column;align-items:center;justify-content:center;gap:2px;border-radius:var(--u4-r-3);border:1px solid rgba(0,0,0,.35);background:{color};color:{text};cursor:pointer;padding:0 6px;min-width:0;overflow:hidden;width:100%"
  >
    {#if iconDef}
      <svg
        width={iconSize}
        height={iconSize}
        viewBox={iconDef.viewBox}
        fill="none"
        stroke="currentColor"
        stroke-width={iconDef.strokeWidth}
        stroke-linecap={iconDef.linecap ?? 'butt'}
        stroke-linejoin={iconDef.linejoin ?? 'miter'}
        aria-hidden="true"
        style="flex:none"
      >
        {@html iconDef.body}
      </svg>
    {/if}
    <span
      style="display:block;max-width:100%;font-size:{nameSize}px;font-weight:600;line-height:1.1;white-space:nowrap;overflow:hidden;text-overflow:ellipsis"
      >{name}</span
    >
  </button>
{:else}
  <div
    style="height:{h}px;flex:none;display:flex;align-items:center;justify-content:center;border-radius:var(--u4-r-3);background:{color};color:{text};font-size:{nameSize}px;font-weight:600;overflow:hidden"
  >
    {name}
  </div>
{/if}

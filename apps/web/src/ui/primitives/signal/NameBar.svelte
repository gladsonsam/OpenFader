<!-- Channel identity: neutral icon area above a solid colour name label. -->
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
    /** Channel icon name (null = none). Rendered above the name. */
    icon?: string | null;
    variant?: 'strip' | 'plate';
    /** When set, renders as a button with this aria-label (design: 'Select Ch 1, source Lcl 01'). */
    selectAria?: string | null;
    onselect?: (() => void) | null;
  } = $props();

  /** Guarded lookup: fixtures default to null; the store only writes known names. */
  const iconDef = $derived(icon != null && icon in chanIcons ? chanIcons[icon as ChanIconName] : null);
  const iconSize = 44;
  const h = $derived(iconDef ? iconSize + 16 + 32 : variant === 'plate' ? 54 : 76);
  const nameSize = $derived(variant === 'plate' ? 17 : 16);
</script>

{#snippet content()}
  <span class="icon-area" aria-hidden="true">
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
      >
        {@html iconDef.body}
      </svg>
    {/if}
  </span>
  <span class="name-label" style:font-size={`${nameSize}px`}>
    <span class="name-text">{name}</span>
  </span>
{/snippet}

{#if selectAria}
  <button
    data-ctl="name"
    data-variant={variant}
    style:--channel-color={color}
    style:--channel-ink={text}
    style:height={`${h}px`}
    aria-label={selectAria}
    class="namebar tpress"
    onclick={onselect
      ? (e) => {
          // Don't bubble to a selectable ancestor (mixer strip): the name
          // tap opens the editor, and the strip select firing after it
          // would close the editor again immediately.
          e.stopPropagation();
          onselect();
        }
      : undefined}
  >
    {@render content()}
  </button>
{:else}
  <div
    data-ctl="name"
    data-variant={variant}
    style:--channel-color={color}
    style:--channel-ink={text}
    style:height={`${h}px`}
    class="namebar"
  >
    {@render content()}
  </div>
{/if}

<style>
  .namebar {
    flex: none;
    display: flex;
    flex-direction: column;
    width: 100%;
    min-width: 0;
    padding: 0;
    border: 0;
    border-radius: var(--u4-r-3);
    overflow: hidden;
    background: var(--u4-panel-bg);
    color: var(--u4-text-4);
  }
  button.namebar { cursor: pointer; }
  .icon-area {
    flex: 1;
    min-height: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 8px 0;
  }
  .name-label {
    flex: none;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 100%;
    height: 32px;
    padding: 0 6px;
    background: var(--channel-color);
    color: var(--channel-ink);
    font-weight: 600;
    line-height: 1.2;
  }
  .name-text {
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  button.namebar:active .icon-area {
    background: var(--u4-tile-bg-on);
  }
</style>

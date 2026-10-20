<!-- W5c module `strip-tag`: the phone-only strip label (name + channel
      icon stacked, like the desktop NameBar but compact for the phone snap
      pager). Taps emit `select` for the layout `on` action (phone: ui.select
      + open the channel page, same as `name-open`). -->
<script lang="ts">
  import { chanIcons, type ChanIconName } from '../../icons.ts';
  import type { ModuleContext } from '../../modules/types.ts';

  let {
    props = {},
    ctx: _ctx,
    nodeId: _nodeId,
    onEvent,
  }: {
    props: {
      name?: string;
      sub?: string;
      color?: string;
      text?: string;
      icon?: string | null;
      selectAria?: string | null;
    };
    ctx: ModuleContext;
    nodeId: string;
    onEvent?: (name: string, payload?: unknown) => void;
  } = $props();

  /** Guarded lookup: fixtures default to null; the store only writes known names. */
  const iconDef = $derived(
    props.icon != null && props.icon in chanIcons ? chanIcons[props.icon as ChanIconName] : null,
  );
  // 32px icon + 2px gap + 13px name@1.1 fits inside 56px with room to spare.
  const iconSize = 32;
</script>

<button
  aria-label={props.selectAria ?? props.name ?? ''}
  class="tpress"
  onclick={(e) => {
    // Same stop-propagation contract as NameBar: the tap selects via the
    // layout action, it must not also trigger the selectable strip ancestor.
    e.stopPropagation();
    onEvent?.('select');
  }}
  style="height:56px;flex:none;display:flex;flex-direction:column;align-items:center;justify-content:center;gap:2px;border-radius:var(--u4-r-3);border:1px solid rgba(0,0,0,.35);background:{props.color ??
    'var(--u4-line-faint)'};color:{props.text ?? 'var(--u4-text-1)'};cursor:pointer;padding:0 6px;min-width:0;overflow:hidden;width:100%"
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
    style="display:block;max-width:100%;font-size:13px;font-weight:600;line-height:1.1;white-space:nowrap;overflow:hidden;text-overflow:ellipsis"
    >{props.name ?? ''}</span
  >
</button>

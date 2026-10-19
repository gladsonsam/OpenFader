<!-- W5c module `strip-tag`: the phone-only tiny strip label (~28px single
      line: name on the strip colour). Presentational like `strip-name` but
      fixed-height compact for the phone snap pager, where the 76px NameBar
      eats too much vertical throw. Taps emit `select` for the layout `on`
      action (phone: ui.select + open the channel page, same as `name-open`). -->
<script lang="ts">
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
      selectAria?: string | null;
    };
    ctx: ModuleContext;
    nodeId: string;
    onEvent?: (name: string, payload?: unknown) => void;
  } = $props();
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
  style="height:28px;flex:none;display:flex;align-items:center;justify-content:center;border-radius:var(--u4-r-3);border:1px solid rgba(0,0,0,.35);background:{props.color ??
    'var(--u4-line-faint)'};color:{props.text ?? 'var(--u4-text-1)'};cursor:pointer;padding:0 6px;min-width:0;overflow:hidden;width:100%"
>
  <span
    style="display:block;max-width:100%;font-size:13px;font-weight:600;line-height:1.1;white-space:nowrap;overflow:hidden;text-overflow:ellipsis"
    >{props.name ?? ''}</span
  >
</button>

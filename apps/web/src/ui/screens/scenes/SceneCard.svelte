<!-- S910 module `scene-card`: one scene tile (`001` number, bold name,
      `Current` tag + green border on the live scene, amber border when
      preview-selected). L1 module form: card fields in via layout `bind`,
      `select` event out with payload {index} (the layout maps it to
      ui.pickScene). Presentational: props in, callbacks out. Inline styles
      verbatim from docs/design/src/design-template.html (scenes card);
      button border-box + font-variant-numeric:normal like the design. -->
<script lang="ts">
  import type { ModuleContext } from '../../modules/types.ts';

  let {
    props = {},
    ctx: _ctx,
    nodeId: _nodeId,
    onEvent,
  }: {
    props: {
      index?: number;
      num?: string;
      name?: string;
      tag?: string;
      pressed?: boolean;
      border?: string;
      bg?: string;
    };
    ctx: ModuleContext;
    nodeId: string;
    onEvent?: (name: string, payload?: unknown) => void;
  } = $props();
</script>

<button
  onclick={() => onEvent?.('select', { index: props.index ?? 0 })}
  aria-pressed={props.pressed ? 'true' : 'false'}
  style="box-sizing:border-box;font-variant-numeric:normal;height:84px;display:flex;flex-direction:column;justify-content:space-between;padding:10px 12px;border-radius:3px;border:2px solid {props.border ??
    '#222a36'};background:{props.bg ?? '#0e1520'};color:#eef2f7;cursor:pointer;text-align:left"
>
  <span style="box-sizing:content-box;display:flex;justify-content:space-between;align-items:center;gap:8px">
    <span style="box-sizing:content-box;font-size:13px;color:#a7b1c0">{props.num ?? ''}</span>
    <span style="box-sizing:content-box;font-size:11px;font-weight:600;color:#7be27b">{props.tag ?? ''}</span>
  </span>
  <span style="box-sizing:content-box;font-size:18px;font-weight:600;white-space:nowrap;overflow:hidden;text-overflow:ellipsis">{props.name ?? ''}</span>
</button>

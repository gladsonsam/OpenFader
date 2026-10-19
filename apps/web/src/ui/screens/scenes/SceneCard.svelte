<!-- S910 module `scene-card`: one scene tile (`001` number, bold name,
      `Current` tag + green border on the live scene, amber border when
      preview-selected). L1 module form: card fields in via layout `bind`,
      `select` event out with payload {index} (the layout maps it to
      ui.pickScene). Presentational: props in, callbacks out. Inline styles
      verbatim from docs/design/src/design-template.html (scenes card);
      button border-box + font-variant-numeric:normal like the design. -->
<script lang="ts">
  import type { ModuleContext } from '../../modules/types.ts';
  import { useDeviceClass } from '../../modules/device.svelte.ts';

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

  // Phone density: single-column list rows slim from 84px to 60px so all
  // eight scenes fit without scrolling. Desktop keeps the design-exact 84px.
  const dev = useDeviceClass();
  const phone = $derived(dev.now === 'phone');
</script>

<button
  onclick={() => onEvent?.('select', { index: props.index ?? 0 })}
  aria-pressed={props.pressed ? 'true' : 'false'}
  style="box-sizing:border-box;font-variant-numeric:normal;height:{phone ? 60 : 84}px;display:flex;flex-direction:column;justify-content:space-between;padding:{phone
    ? '6px 12px'
    : '10px 12px'};border-radius:3px;border:2px solid {props.border ??
    'var(--u4-line-faint)'};background:{props.bg ?? 'var(--u4-panel-bg)'};color:var(--u4-text-2);cursor:pointer;text-align:left"
>
  <span style="box-sizing:content-box;display:flex;justify-content:space-between;align-items:center;gap:8px">
    <span style="box-sizing:content-box;font-size:13px;color:var(--u4-text-5)">{props.num ?? ''}</span>
    <span style="box-sizing:content-box;font-size:11px;font-weight:600;color:var(--u4-green-hi)">{props.tag ?? ''}</span>
  </span>
  <span style="box-sizing:content-box;font-size:{phone ? 16 : 18}px;font-weight:600;white-space:nowrap;overflow:hidden;text-overflow:ellipsis">{props.name ?? ''}</span>
</button>

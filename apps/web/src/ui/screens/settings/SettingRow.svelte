<!-- S910 module `setting-row`: one Settings row (title + description left,
      option buttons right, hairline separator). L1 module form: props in via
      layout `bind` (one row of the descriptor-driven settingsVm rows), `change`
      event out with payload {key, value} (the layout maps it to ui.setSetting).
      Rendering is uniform option buttons for segmented/toggle/pads: the 16
      send-target pads use the same button styles verbatim from
      docs/design/src/design-template.html (settings seg buttons), so the mix
      row binds its 16 pad options and renders identically. `none` rows
      (About) bind options [] and render the empty group, like the template.
      Inline styles otherwise verbatim from the template (settings row +
      option buttons); box model: divs content-box, buttons border-box (the
      design's browser defaults); buttons set font-variant-numeric:normal. -->
<script lang="ts">
  import type { ModuleContext } from '../../modules/types.ts';
  import type { SettingOptionVm } from './settingsVm.ts';

  let {
    props = {},
    ctx: _ctx,
    nodeId: _nodeId,
    onEvent,
  }: {
    props: {
      rowKey?: string;
      label?: string;
      desc?: string;
      control?: string;
      options?: SettingOptionVm[];
    };
    ctx: ModuleContext;
    nodeId: string;
    onEvent?: (name: string, payload?: unknown) => void;
  } = $props();
</script>

<div style="box-sizing:content-box;display:flex;flex-wrap:wrap;align-items:center;gap:10px;padding:12px 14px;border-top:1px solid var(--u4-line-row)">
  <div style="box-sizing:content-box;flex:1 1 180px;min-width:0">
    <div style="box-sizing:content-box;font-size:15px;font-weight:600">{props.label ?? ''}</div>
    <div style="box-sizing:content-box;font-size:13px;color:var(--u4-text-7)">{props.desc ?? ''}</div>
  </div>
  <div role="group" aria-label={props.label ?? ''} style="box-sizing:content-box;display:flex;flex-wrap:wrap;gap:4px;justify-content:flex-end">
    {#each props.options ?? [] as o (o.value)}
      <button
        onclick={() => onEvent?.('change', { key: props.rowKey, value: o.value })}
        aria-pressed={o.pressed ? 'true' : 'false'}
        style="box-sizing:border-box;font-variant-numeric:normal;height:40px;min-width:52px;padding:0 12px;border-radius:3px;border:1px solid {o.border};background:{o.bg};color:{o.fg};font-size:14px;cursor:pointer"
      >{o.label}</button>
    {/each}
  </div>
</div>

<!-- F1 module `fx-slot-card`: one FX slot card button. L1 module form: slot
      fields in via layout `bind`, `select` event out with payload {index}
      (the layout maps it to ui.setFxSel). Presentational: props in, callbacks
      out. Inline styles from the design template (fxSlots card): h 96, 2px
      border, name/state row, type + return meter bar. The type row shows the
      family art swatch + full display name (desk short code as a sub-caption
      where one exists), so phone + desktop both read names instead of codes.
      Keyboard operable native button. -->
<script lang="ts">
  import type { ModuleContext } from '../../modules/types.ts';
  import type { FxArt } from './fxModel.ts';

  let {
    props = {},
    ctx: _ctx,
    nodeId: _nodeId,
    onEvent,
  }: {
    props: {
      index?: number;
      name?: string;
      type?: string;
      code?: string;
      art?: FxArt;
      state?: string;
      stateColor?: string;
      ret?: string;
      pressed?: boolean;
      border?: string;
      bg?: string;
      meterPct?: string;
    };
    ctx: ModuleContext;
    nodeId: string;
    onEvent?: (name: string, payload?: unknown) => void;
  } = $props();
</script>

<button
  onclick={() => onEvent?.('select', { index: props.index ?? 0 })}
  aria-pressed={props.pressed ? 'true' : 'false'}
  style="box-sizing:border-box;font-variant-numeric:normal;height:96px;display:flex;flex-direction:column;justify-content:space-between;gap:4px;padding:10px;border-radius:3px;border:2px solid {props.border ??
    'var(--u4-line-faint)'};background:{props.bg ?? 'var(--u4-panel-bg)'};color:{props.pressed
    ? 'var(--u4-selected-ink, #eef2f7)'
    : 'var(--u4-text-2)'};cursor:pointer;text-align:left"
>
  <span style="display:flex;justify-content:space-between;align-items:center">
    <span style="font-size:12px;color:var(--u4-text-5)">{props.name ?? ''}</span>
    <span style="font-size:11px;font-weight:600;color:{props.stateColor ?? '#7be27b'}">{props.state ?? ''}</span>
  </span>
  <span style="display:flex;align-items:center;gap:8px;min-width:0">
    <span
      aria-hidden="true"
      style="flex:none;width:26px;height:26px;border-radius:6px;background:{props.art?.gradient ??
        'var(--u4-line-ctl)'};color:#fff;font-size:13px;line-height:26px;text-align:center"
      >{props.art?.glyph ?? ''}</span
    >
    <span style="flex:1;min-width:0;display:flex;flex-direction:column;line-height:1.2">
      <span
        title={props.code ? (props.type ?? '') + ' (' + props.code + ')' : (props.type ?? '')}
        style="font-size:16px;font-weight:600;overflow:hidden;text-overflow:ellipsis;white-space:nowrap"
        >{props.type ?? ''}</span
      >
      {#if props.code}<span style="font-size:10px;opacity:0.65;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">{props.code} · {props.art?.category ?? ''}</span>{/if}
    </span>
  </span>
  <span style="display:flex;align-items:center;gap:8px">
    <span style="font-size:12px;color:var(--u4-text-7);white-space:nowrap">{props.ret ?? ''}</span>
    <span
      style="flex:1;height:4px;position:relative;overflow:hidden;border-radius:1px;background:#1a2332"
      ><span
        style="position:absolute;left:0;top:0;bottom:0;width:{props.meterPct ?? '0'}%;background:#b28cff"
      ></span></span
    >
  </span>
</button>

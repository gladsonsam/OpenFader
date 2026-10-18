<!-- S910 module `scene-recall-bar`: the bottom bar (`Current scene` green
      label + preview number/name, Cancel + Recall). L1 module form: recall
      fields in via layout `bind`, `cancel` / `recall` events out (the layout
      maps them to ui.goBack / ui.recallScene). Presentational: props in,
      callbacks out. The Recall button is never `disabled`: like the design it
      stays clickable and reports aria-disabled, and recallScene no-ops when
      nothing is pending. Inline styles verbatim from
      docs/design/src/design-template.html (scenes recall bar). -->
<script lang="ts">
  import type { ModuleContext } from '../../modules/types.ts';

  let {
    props = {},
    ctx: _ctx,
    nodeId: _nodeId,
    onEvent,
  }: {
    props: {
      tag?: string;
      tagColor?: string;
      num?: string;
      name?: string;
      barBorder?: string;
      border?: string;
      bg?: string;
      text?: string;
      disabled?: boolean;
    };
    ctx: ModuleContext;
    nodeId: string;
    onEvent?: (name: string, payload?: unknown) => void;
  } = $props();
</script>

<div style="box-sizing:content-box;flex:none;display:flex;align-items:center;gap:8px;padding:8px;border-radius:3px;border:1px solid {props.barBorder ??
  '#222a36'};background:#0e1520">
  <div style="box-sizing:content-box;flex:1;min-width:0;line-height:1.2">
    <div style="box-sizing:content-box;font-size:12px;color:{props.tagColor ?? '#7be27b'}">{props.tag ?? ''}</div>
    <div style="box-sizing:content-box;font-size:16px;font-weight:600;white-space:nowrap;overflow:hidden;text-overflow:ellipsis"><span style="box-sizing:content-box;font-weight:500;color:#a7b1c0;margin-right:6px">{props.num ?? ''}</span>{props.name ?? ''}</div>
  </div>
  <button
    onclick={() => onEvent?.('cancel')}
    style="box-sizing:border-box;font-variant-numeric:normal;height:48px;padding:0 20px;border-radius:3px;border:1px solid #3a414c;background:#2b3038;color:#e8edf5;font-size:15px;cursor:pointer"
  >Cancel</button>
  <button
    onclick={() => onEvent?.('recall')}
    aria-disabled={props.disabled ? 'true' : 'false'}
    style="box-sizing:border-box;font-variant-numeric:normal;height:48px;padding:0 28px;border-radius:3px;border:1px solid {props.border ??
      '#2a323f'};background:{props.bg ?? '#161c26'};color:{props.text ?? '#5d6878'};font-size:15px;font-weight:600;cursor:pointer"
  >Recall</button>
</div>

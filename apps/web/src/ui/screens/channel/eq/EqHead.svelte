<!-- W5b `eq-head`: the EQ tab head row buttons (EQ In / Bypassed toggle +
     Flat). Presentational: toggle visuals in via layout `bind`, `toggle` /
     `flat` events out. Button chrome verbatim from design-template.html
     (40px, radius 5, 15px) — the generic `btn` leaf hard-codes radius 3,
     so these two buttons stay a dedicated channel module. -->
<script lang="ts">
  import type { ModuleContext } from '../../../modules/types.ts';

  let {
    props = {},
    ctx: _ctx,
    nodeId: _nodeId,
    onEvent,
  }: {
    props: {
      label?: string;
      bg?: string;
      fg?: string;
      pressed?: string;
      ariaLabel?: string;
    };
    ctx: ModuleContext;
    nodeId: string;
    onEvent?: (name: string, payload?: unknown) => void;
  } = $props();
</script>

<div style="box-sizing:content-box;display:flex;align-items:center;gap:8px">
  <button
    onclick={() => onEvent?.('toggle')}
    aria-pressed={props.pressed ?? 'false'}
    aria-label={props.ariaLabel ?? 'EQ in'}
    style="box-sizing:border-box;font-variant-numeric:normal;height:40px;padding:0 16px;border-radius:5px;border:1px solid var(--u4-line-ctl);background:{props.bg ??
      'var(--u4-btn-bg)'};color:{props.fg ?? 'var(--u4-text-1)'};font-size:15px;cursor:pointer"
    >{props.label ?? ''}</button
  >
  <button
    onclick={() => onEvent?.('flat')}
    aria-label="Flatten EQ gains"
    style="box-sizing:border-box;font-variant-numeric:normal;height:40px;padding:0 16px;border-radius:5px;border:1px solid var(--u4-line-ctl);background:var(--u4-btn-bg);color:var(--u4-text-1);font-size:15px;cursor:pointer"
    >Flat</button
  >
</div>

<!-- F1 module `fx-type-selector`: the row of effect-type buttons. L1 module
      form: the type button vms in via layout `bind` (a repeat of buttons over
      the type list), `pickType` event out with payload {type} (the wire key:
      caps.fx id online, design label offline — the layout maps it to
      ui.setFxType). Group role + label and grid layout from the design
      template (auto-fit minmax, gap 6); buttons show the family art dot +
      full display name with the desk short code as a sub-caption where one
      exists (xair), so phone + desktop both read names instead of codes. -->
<script lang="ts">
  import type { ModuleContext } from '../../modules/types.ts';
  import type { FxTypeVm } from './fxModel.ts';

  let {
    props = {},
    ctx: _ctx,
    nodeId: _nodeId,
    onEvent,
  }: {
    props: {
      types?: FxTypeVm[];
    };
    ctx: ModuleContext;
    nodeId: string;
    onEvent?: (name: string, payload?: unknown) => void;
  } = $props();
</script>

<div
  role="group"
  aria-label="Effect type"
  style="box-sizing:content-box;display:grid;grid-template-columns:repeat(auto-fit, minmax(132px, 1fr));gap:6px"
>
  {#each props.types ?? [] as t (t.value || t.label)}
    <button
      onclick={() => onEvent?.('pickType', { type: t.value || t.label })}
      aria-pressed={t.pressed ? 'true' : 'false'}
      title={t.code ? t.label + ' (' + t.code + ')' : t.label}
      style="box-sizing:border-box;font-variant-numeric:normal;min-height:44px;display:flex;align-items:center;gap:8px;padding:4px 10px;border-radius:3px;border:1px solid var(--u4-line-ctl);background:{t.bg};color:{t.fg};font-size:14px;cursor:pointer;text-align:left"
      ><span
        aria-hidden="true"
        style="flex:none;width:22px;height:22px;border-radius:50%;background:{t.art?.gradient ??
          'var(--u4-line-ctl)'};color:#fff;font-size:12px;line-height:22px;text-align:center"
        >{t.art?.glyph ?? ''}</span
      ><span style="flex:1;min-width:0;display:flex;flex-direction:column;justify-content:center;line-height:1.25"
        ><span style="overflow:hidden;text-overflow:ellipsis;white-space:nowrap">{t.label}</span
        >{#if t.code}<span style="font-size:10px;opacity:0.65;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">{t.code}</span>{/if}</span
      ></button
    >
  {/each}
</div>

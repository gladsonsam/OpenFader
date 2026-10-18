<!-- F1 module `fx-type-selector`: the row of effect-type buttons. L1 module
      form: the type button vms in via layout `bind` (a repeat of buttons over
      the FX_TYPES-derived list), `pickType` event out with payload {type}
      (the layout maps it to ui.setFxType). Group role + label and button
      styles verbatim from the design template (auto-fit minmax 96px, h 44;
      selected = purple bg + dark text). -->
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
  style="box-sizing:content-box;display:grid;grid-template-columns:repeat(auto-fit, minmax(96px, 1fr));gap:6px"
>
  {#each props.types ?? [] as t (t.label)}
    <button
      onclick={() => onEvent?.('pickType', { type: t.label })}
      aria-pressed={t.pressed ? 'true' : 'false'}
      style="box-sizing:border-box;font-variant-numeric:normal;height:44px;border-radius:3px;border:1px solid #3a414c;background:{t.bg};color:{t.fg};font-size:14px;cursor:pointer"
      >{t.label}</button
    >
  {/each}
</div>

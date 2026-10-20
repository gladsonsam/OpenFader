<!-- W5c module `strip-well-empty`: the DCA proc-well placeholder (strips with
     no EQ/Dyn: no eq/dyn in the data). Verbatim from design-template.html
     (content-box, so the flex height + border matches to the px). Shown by
     the layout `when: '!item.hasEq'` next to the EQ/Dyn well. -->
<script lang="ts">
  import type { ModuleContext } from '../../modules/types.ts';

  let {
    props: _props = {},
    ctx: _ctx,
    nodeId: _nodeId,
    onEvent: _onEvent,
  }: {
    props: Record<string, unknown>;
    ctx: ModuleContext;
    nodeId: string;
    onEvent?: (name: string, payload?: unknown) => void;
  } = $props();
</script>

<div
  aria-hidden="true"
  style="flex:none;height:102px;border:1px solid var(--u4-line-faint);border-radius:3px;background:var(--u4-well-bg);box-sizing:border-box"
></div>

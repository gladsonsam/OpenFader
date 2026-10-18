<!-- L0 missing-module placeholder: visible box so saved profiles built for
     another build/desk still load AND show what is missing (never silently
     dropped; the editor flags it "not available"). -->
<script lang="ts">
  import type { ModuleContext } from '../types.ts';

  let {
    props = {},
    ctx: _ctx,
    nodeId,
  }: {
    props: { missing?: string };
    ctx: ModuleContext;
    nodeId: string;
  } = $props();
</script>

<div
  data-l0="missing-module"
  data-node={nodeId}
  role="note"
  aria-label={`Module not available: ${props.missing ?? 'unknown'}`}
  style="display:block;border:1px dashed #f0b44a;color:#f0b44a;font-size:11px;padding:4px 6px"
>
  missing: {props.missing ?? 'unknown'}
</div>

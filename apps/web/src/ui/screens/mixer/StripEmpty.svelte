<!-- W5c module `strip-empty`: one empty bank-slot placeholder (short banks:
     mtx has 10 items -> 6 of these). Verbatim from design-template.html. -->
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

<div style="background:#0a0e15;border-radius:0"></div>

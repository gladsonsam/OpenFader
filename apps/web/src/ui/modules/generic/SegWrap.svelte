<!-- L0 generic segmented module: presentational wrapper around ui Segmented.
     `options` is a JSON-friendly string list; selection is display-only here. -->
<script lang="ts">
  import Segmented from '../../primitives/Segmented.svelte';
  import type { ModuleContext } from '../types.ts';

  let {
    props = {},
    ctx: _ctx,
    nodeId: _nodeId,
  }: {
    props: { groupLabel?: string; options?: string[]; selected?: string };
    ctx: ModuleContext;
    nodeId: string;
  } = $props();

  const opts = $derived((props.options ?? []).map((o) => ({ value: o, label: o, selected: o === props.selected })));
</script>

<Segmented groupLabel={props.groupLabel ?? 'Options'} options={opts} onpick={() => {}} />

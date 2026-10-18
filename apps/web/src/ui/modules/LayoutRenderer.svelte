<!-- L0 LayoutRenderer: renders a layout JSON tree via RenderNode.
     Lenient: the tree is normalised first (unknown modules -> visible
     `missing-module` placeholders), so saved profiles from another
     build/desk still load. Adds no wrapper elements of its own. -->
<script lang="ts">
  import RenderNode from './RenderNode.svelte';
  import { getModule } from './registry.ts';
  import { normalizeLayout } from './validate.ts';
  import type { LayoutNode, RepeatScope } from './schema.ts';
  import type { ContextSource } from './context.ts';
  import type { DeviceClass, Role } from './types.ts';

  let {
    layout,
    source,
    scope = {},
    device = 'desktop',
    role = 'engineer',
    caps = [],
    settings = {},
  }: {
    layout: LayoutNode;
    source: ContextSource;
    scope?: RepeatScope;
    device?: DeviceClass;
    role?: Role;
    caps?: string[];
    settings?: Record<string, unknown>;
  } = $props();

  const tree = $derived(normalizeLayout(layout, (id) => getModule(id) !== undefined).tree);
</script>

<RenderNode node={tree} {source} {scope} {device} {role} {caps} {settings} />

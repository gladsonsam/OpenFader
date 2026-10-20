<!-- W5c module `strip-frame`: one filled bank slot. The L1 repeat exposes the
     row object as `item` on the template instance only; the strip anatomy
     (a container subtree) needs it a level deeper, so the frame re-roots the
     user-editable strip anatomy (strip.default.json) in a nested
     LayoutRenderer with `{ store, item: cell, index, frozen }` scope. The
     frame itself renders no box: the anatomy's root column is the grid item.
     The anatomy tree arrives via `bind` (the screen derives it through the
     mixer profile store, so user edits apply); per-strip data arrives as the
     `cell` view-model (meter feed attached); drags/taps dispatch through the
     global action registry exactly like top-level nodes. -->
<script lang="ts">
  import LayoutRenderer from '../../modules/LayoutRenderer.svelte';
  import { fixtureSource } from '../../modules/context.ts';
  import type { LayoutNode } from '../../modules/schema.ts';
  import type { DeviceClass } from '../../modules/types.ts';
  import type { ModuleContext } from '../../modules/types.ts';

  let {
    props = {},
    ctx,
    nodeId: _nodeId,
    onEvent: _onEvent,
  }: {
    props: {
      cell?: unknown;
      index?: number;
      store?: unknown;
      frozen?: boolean;
      tree?: LayoutNode;
      /** Device class bound from the parent scope (`bind: { device: 'device' }`
          in mixer.default.json) so the user-editable strip anatomy applies
          its own phone/tablet variants inside this nested renderer. */
      device?: DeviceClass;
    };
    ctx: ModuleContext;
    nodeId: string;
    onEvent?: (name: string, payload?: unknown) => void;
  } = $props();

  const fallback: LayoutNode = { id: 'strip-missing', module: 'strip-empty' };
  const layout = $derived(props.tree ?? fallback);
  const device = $derived(props.device ?? 'desktop');
  const scope = $derived({
    store: props.store,
    item: props.cell,
    index: props.index ?? 0,
    frozen: props.frozen ?? false,
  });
  const source = fixtureSource({ meter: ref => ctx.useMeter(`meter:${ref}`) });
</script>

<LayoutRenderer {layout} {source} {scope} {device} settings={{}} />

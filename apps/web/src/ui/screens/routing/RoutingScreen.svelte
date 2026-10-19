<!-- L1 Routing screen: rendered 100% from ui/layouts/routing.default.json
     through LayoutRenderer + actions + bindings. The JSON owns the
     composition (cards, chips, tile grids); view-model rows flow in through
     `bind` expressions; clicks dispatch `ui.*` actions through the store.
     This file only registers the layout: scope + profile + action binding. -->
<script lang="ts">
  import { ui } from '../../state/uiStore.svelte.ts';
  import { live } from '../../state/live.svelte.ts';
  import { useDeviceClass } from '../../modules/device.svelte.ts';
  import { bridgeStrip } from '../../state/engineBridge.ts';
  import LayoutRenderer from '../../modules/LayoutRenderer.svelte';
  import { fixtureSource } from '../../modules/context.ts';
  import { createProfileStore } from '../../modules/profile/store.svelte.ts';
  import { bindUiActions } from '../../modules/actions.ts';
  import { buildRoutingVm } from './routingVm.ts';
  import type { LayoutNode } from '../../modules/schema.ts';
  import defaults from '../../layouts/routing.default.json';

  const store = createProfileStore('routing');
  bindUiActions(ui.do, { profile: store });

  // Phone rework: the layout gates the matrix (`device != 'phone'`) and the
  // list/picker patcher (`device == 'phone'`) on the live device class, so it
  // has to reach the renderer (desktop default keeps fixture baselines exact).
  const device = useDeviceClass();

  // Live patch overlay (item 9): online with a supporting driver, engine
  // patches replace the fixture source labels (desk truth wins); xair
  // reports routingSupported:false so the fixture labels stay with an
  // honest note from the vm. Staged edits still flow through ui.do.
  function liveSrc(): Record<string, string> | null {
    void live.version;
    const w = live.engine;
    if (live.phase !== 'online' || !w || !w.capsReady || !w.routingSupported) return null;
    const out: Record<string, string> = {};
    for (const p of w.patches) {
      const kind = w.kindOf(p.strip);
      if (!kind) continue;
      let n = 0;
      for (let c = 0; c < p.strip; c++) if (w.kindOf(c) === kind) n++;
      const id = kind === 'in' ? `in${n + 1}` : null;
      if (id) out[id] = `${p.group} ${String(p.index).padStart(2, '0')}`;
    }
    return out;
  }

  const vm = $derived(
    buildRoutingVm(ui.state, ui.chans, (() => {
      const over = liveSrc();
      if (!over) return undefined;
      return over;
    })()),
  );
  const tree = $derived(store.tree(defaults as unknown as LayoutNode));
  const scope = $derived({
    store: ui.state,
    routing: vm,
    lists: {
      routeBanks: vm.banks,
      routeTiles: vm.tiles,
      routeGroups: vm.groups,
      srcTiles: vm.srcTiles,
    },
  });
  const source = fixtureSource({});
</script>

<LayoutRenderer layout={tree} {source} {scope} device={device.now} />

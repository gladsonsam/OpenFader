<!-- L1 Routing screen: rendered 100% from ui/layouts/routing.default.json
     through LayoutRenderer + actions + bindings. The JSON owns the
     composition (cards, chips, tile grids); view-model rows flow in through
     `bind` expressions; clicks dispatch `ui.*` actions through the store.
     This file only registers the layout: scope + profile + action binding. -->
<script lang="ts">
  import { ui } from '../../state/uiStore.svelte.ts';
  import LayoutRenderer from '../../modules/LayoutRenderer.svelte';
  import { fixtureSource } from '../../modules/context.ts';
  import { createProfileStore } from '../../modules/profile/store.svelte.ts';
  import { bindUiActions } from '../../modules/actions.ts';
  import { buildRoutingVm } from './routingVm.ts';
  import type { LayoutNode } from '../../modules/schema.ts';
  import defaults from '../../layouts/routing.default.json';

  const store = createProfileStore('routing');
  bindUiActions(ui.do, { profile: store });

  const vm = $derived(buildRoutingVm(ui.state, ui.chans));
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

<LayoutRenderer layout={tree} {source} {scope} />

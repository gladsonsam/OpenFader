<!-- L1 Connection screen: rendered 100% from
     ui/layouts/connection.default.json through LayoutRenderer + actions +
     bindings (status card, device rows, offline card). This file only
     registers the layout: scope + profile + action binding. -->
<script lang="ts">
  import { ui } from '../../state/uiStore.svelte.ts';
  import LayoutRenderer from '../../modules/LayoutRenderer.svelte';
  import { fixtureSource } from '../../modules/context.ts';
  import { createProfileStore } from '../../modules/profile/store.svelte.ts';
  import { bindUiActions } from '../../modules/actions.ts';
  import { buildConnectionVm } from './connectionVm.ts';
  import type { LayoutNode } from '../../modules/schema.ts';
  import defaults from '../../layouts/connection.default.json';

  const store = createProfileStore('connection');
  bindUiActions(ui.do, { profile: store });

  const vm = $derived(buildConnectionVm(ui.state));
  const tree = $derived(store.tree(defaults as unknown as LayoutNode));
  const scope = $derived({
    store: ui.state,
    conn: vm,
    lists: { devices: vm.devices },
  });
  const source = fixtureSource({});
</script>

<LayoutRenderer layout={tree} {source} {scope} />

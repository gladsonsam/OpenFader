<!-- S910 Scenes screen: rendered 100% from ui/layouts/scenes.default.json
      through LayoutRenderer + actions + bindings. The JSON owns the
      composition (scrollable card grid via repeat, recall bar); card and
      recall view-models flow in through `bind` expressions; taps dispatch
      ui.pickScene (Instant loads immediately, Confirm previews), Cancel/Recall
      dispatch ui.goBack/ui.recallScene through the store. This file only
      registers the layout: scope + profile + action binding. -->
<script lang="ts">
  import { ui } from '../../state/uiStore.svelte.ts';
  import LayoutRenderer from '../../modules/LayoutRenderer.svelte';
  import { fixtureSource } from '../../modules/context.ts';
  import { createProfileStore } from '../../modules/profile/store.svelte.ts';
  import { bindUiActions } from '../../modules/actions.ts';
  import { buildScenesVm } from './scenesVm.ts';
  import type { LayoutNode } from '../../modules/schema.ts';
  import defaults from '../../layouts/scenes.default.json';

  const store = createProfileStore('scenes');
  bindUiActions(ui.do, { profile: store });

  const vm = $derived(buildScenesVm(ui.state));
  const tree = $derived(store.tree(defaults as unknown as LayoutNode));
  const scope = $derived({
    store: ui.state,
    recall: vm.recall,
    lists: { sceneCards: vm.cards },
  });
  const source = fixtureSource({});
</script>

<LayoutRenderer layout={tree} {source} {scope} />

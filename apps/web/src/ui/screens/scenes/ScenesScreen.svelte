<!-- S910 Scenes screen: rendered 100% from ui/layouts/scenes.default.json
      through LayoutRenderer + actions + bindings. The JSON owns the
      composition (scrollable card grid via repeat, recall bar); card and
      recall view-models flow in through `bind` expressions; taps dispatch
      ui.pickScene (Instant loads immediately, Confirm previews), Cancel/Recall
      dispatch ui.goBack/ui.recallScene through the store. This file only
      registers the layout: scope + profile + action binding. -->
<script lang="ts">
  import { ui } from '../../state/uiStore.svelte.ts';
  import { live } from '../../state/live.svelte.ts';
  import LayoutRenderer from '../../modules/LayoutRenderer.svelte';
  import { fixtureSource } from '../../modules/context.ts';
  import { createProfileStore } from '../../modules/profile/store.svelte.ts';
  import { bindUiActions } from '../../modules/actions.ts';
  import { buildScenesVm } from './scenesVm.ts';
  import SnapshotsPanel from './SnapshotsPanel.svelte';
  import type { LayoutNode } from '../../modules/schema.ts';
  import defaults from '../../layouts/scenes.default.json';

  const store = createProfileStore('scenes');
  bindUiActions(ui.do, { profile: store });

  const vm = $derived(buildScenesVm(ui.state));
  const tree = $derived(store.tree(defaults as unknown as LayoutNode));

  // Live snapshots (item 8): engine numbered library + scope matrix.
  // Online only; fixture shows render from the design data either way.
  const snapOnline = $derived(live.phase === 'online' && !!live.engine?.capsReady);
  const snapEntries = $derived(
    (() => {
      void live.version;
      const w = live.engine;
      if (!snapOnline || !w) return [];
      return w.numberedScenes.map(s => ({ number: s.number, name: s.name, current: s.current }));
    })(),
  );
  const snapBanks = $derived(
    (() => {
      void live.version;
      const w = live.engine;
      if (!snapOnline || !w) return [];
      return w.banks.map(b => ({ key: b.key, label: b.label, strips: [...b.strips] }));
    })(),
  );
  function snapSave(name: string) {
    const w = live.engine;
    if (!w) return;
    const used = new Set(w.numberedScenes.map(s => s.number));
    let n = 1;
    while (n <= 999 && used.has(n)) n++;
    if (n > 999) return;
    w.sceneSave(n, name);
  }
  const scope = $derived({
    store: ui.state,
    recall: vm.recall,
    lists: { sceneCards: vm.cards },
  });
  const source = fixtureSource({});
</script>

<LayoutRenderer layout={tree} {source} {scope} />
{#if snapOnline}
  <div style="margin-top:8px">
    <SnapshotsPanel
      entries={snapEntries}
      banks={snapBanks}
      deskName={live.engine?.driver ?? 'desk'}
      onSave={snapSave}
      onLoad={(n, s) => live.engine?.sceneRecall(n, undefined, s)}
      onDelete={n => live.engine?.sceneDelete(n)}
      onClear={() => live.engine?.sceneClear()}
    />
  </div>
{/if}

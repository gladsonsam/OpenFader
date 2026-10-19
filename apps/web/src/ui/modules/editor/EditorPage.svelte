<!-- Layout-editor page (`?ui=4&screen=layout-editor`): the L0 demo layout
     rendered through the generic LayoutRenderer inside the visual-editor
     shell (palette + outline + inspector + share). Same fixture scope as
     Demo.svelte so repeats/settings render; role state previews gates. -->
<script lang="ts">
  import LayoutRenderer from '../LayoutRenderer.svelte';
  import EditorShell from './EditorShell.svelte';
  import { fixtureSource } from '../context.ts';
  import type { LayoutNode, RepeatScope } from '../schema.ts';
  import { bankByKey } from '../../fixtures/designData.ts';
  import { ui } from '../../state/uiStore.svelte.ts';
  import { createProfileStore } from '../profile/store.svelte.ts';
  import { bindUiActions, registerAction } from '../actions.ts';
  import type { Role } from '../types.ts';
  import demoLayout from '../../layouts/l0-demo.default.json';

  const store = createProfileStore('layout-editor');
  const defaults = demoLayout as unknown as LayoutNode;

  let taps = $state(0);
  registerAction('demo.tap', () => {
    taps += 1;
  }, { describe: 'Demo: count a tap' });
  bindUiActions(ui.do, { profile: store });

  let role = $state<Role>('engineer');

  const strips = bankByKey('ch1').items.slice(0, 4).map((i) => i.id);
  const scope = $derived({
    strips,
    bankItems: strips,
    lists: { demo: strips },
    settings: {},
    demo: { taps, theme: 'design-dark' },
  } satisfies RepeatScope & Record<string, unknown>);
  const source = fixtureSource({ strip: (id: string) => ({ id }), settings: {} });
  const tree = $derived(store.tree(defaults));
</script>

<div style="padding:8px">
  <EditorShell {defaults} tree={tree} {store} {role} onRole={(r) => (role = r)}>
    <LayoutRenderer layout={tree} {source} {scope} {role} />
  </EditorShell>
</div>

<!-- L0 gallery demo (?ui=4&screen=l0-demo): small default layout rendered by the
     generic LayoutRenderer, with minimal on-page controls to apply a profile
     op, undo/redo, switch preset theme, export/import JSON. -->
<script lang="ts">
  import LayoutRenderer from '../LayoutRenderer.svelte';
  import { fixtureSource } from '../context.ts';
  import type { LayoutNode, RepeatScope } from '../schema.ts';
  import { bankByKey } from '../../fixtures/designData.ts';
  import { ui } from '../../state/uiStore.svelte.ts';
  import { getDemoStore } from '../profile/store.svelte.ts';
  import { exportProfile, importProfile } from '../profile/io.ts';
  import { bindUiActions, registerAction } from '../actions.ts';
  import { PRESETS, applyTheme, clearTheme, type ThemePresetId } from '../theme.ts';
  import demoLayout from '../../layouts/l0-demo.default.json';

  const store = getDemoStore();
  const defaults = demoLayout as unknown as LayoutNode;

  // L1 demo: a user-inserted working button. `demo.tap` increments taps;
  // the inserted node binds its label to `demo.taps` (computed label) and
  // dispatches `demo.tap` on press (insert op with on.click).
  let taps = $state(0);
  registerAction('demo.tap', () => {
    taps += 1;
  }, { describe: 'Demo: count a tap' });
  bindUiActions(ui.do, { profile: store });

  function insertWorkingButton(): void {
    store.commit({
      op: 'insert',
      parent: 'toolbar',
      index: 99,
      node: {
        id: 'demo-tap-btn',
        module: 'btn',
        props: { label: 'Tap me', height: 40, fontSize: 14 },
        bind: { label: "'taps: ' + demo.taps" },
        on: { press: { do: 'demo.tap' } },
      },
    });
  }

  let rootEl: HTMLDivElement | null = $state(null);
  let appliedVars: string[] = $state([]);
  let activeTheme: ThemePresetId = $state('design-dark');
  let importText = $state('');
  let importError = $state('');
  let helloN = $state(0);
  let panelHidden = $state(false);
  let insertedN = $state(0);

  const strips = bankByKey('ch1').items.slice(0, 4).map((i) => i.id);
  const scope = $derived({
    strips,
    bankItems: strips,
    lists: { demo: strips },
    settings: {},
    demo: { taps, theme: activeTheme },
  } satisfies RepeatScope & Record<string, unknown>);
  const source = fixtureSource({
    strip: (id: string) => ({ id }),
    settings: {},
  });

  const tree = $derived(store.tree(defaults));
  const opCount = $derived(store.profile.ops.length);
  const tapLabel = $derived(`taps: ${taps}`);

  function setTheme(id: ThemePresetId): void {
    activeTheme = id;
    if (rootEl) clearTheme(appliedVars, rootEl);
    appliedVars = rootEl ? applyTheme({ ...PRESETS[id] }, rootEl) : [];
  }

  function doImport(): void {
    const r = importProfile(importText);
    if (!r.ok) {
      importError = r.errors.join('; ');
      return;
    }
    importError = '';
    store.replace(r.profile);
    importText = '';
  }
</script>

<div id="l0-demo" bind:this={rootEl} class="u4-frame" style="padding:8px">
  <div style="display:flex;flex-wrap:wrap;gap:8px;margin-bottom:8px">
    <button
      id="l0-apply"
      onclick={() => {
        helloN += 1;
        store.commit({ op: 'set', id: 'hello', path: 'props.value', value: `hello ${helloN}` });
      }}>Apply op</button
    >
    <button
      id="l0-hide"
      onclick={() => {
        panelHidden = !panelHidden;
        store.commit({ op: 'hide', id: 'panel-a', hidden: panelHidden });
      }}>Toggle hide</button
    >
    <button
      id="l0-insert"
      onclick={() => {
        insertedN += 1;
        store.commit({
          op: 'insert',
          parent: 'toolbar',
          index: 1,
          node: { id: `ins-${insertedN}`, module: 'text', props: { value: `ins ${insertedN}`, size: 12 } },
        });
      }}>Insert</button
    >
    <button
      id="l0-insert-btn"
      onclick={insertWorkingButton}>Insert working button</button
    >
    <button
      id="l0-move"
      onclick={() => {
        const kids = store.tree(defaults).children?.[1]?.children ?? [];
        store.commit({ op: 'move', id: 'btn-a', parent: 'toolbar', index: kids.length });
      }}>Move btn-a to end</button
    >
    <button id="l0-undo" onclick={() => store.undo()} disabled={!store.canUndo}>Undo</button>
    <button id="l0-redo" onclick={() => store.redo()} disabled={!store.canRedo}>Redo</button>
    <button
      id="l0-reset"
      onclick={() => {
        store.reset();
        helloN = 0;
        panelHidden = false;
      }}>Reset</button
    >
    <button id="l0-theme-daylight" onclick={() => setTheme('daylight')}>Daylight</button>
    <button id="l0-theme-night" onclick={() => setTheme('night-red')}>Night red</button>
    <button id="l0-theme-dark" onclick={() => setTheme('design-dark')}>Design dark</button>
    <span id="l0-opcount">ops: {opCount} · theme: {activeTheme} · {tapLabel}</span>
  </div>
  <div style="display:flex;gap:8px;margin-bottom:8px">
    <button id="l0-export" onclick={() => (importText = exportProfile(store.profile))}>Export</button>
    <button id="l0-import" onclick={doImport}>Import</button>
    {#if importError}<span id="l0-import-error" role="alert">{importError}</span>{/if}
  </div>
  <textarea
    id="l0-export-text"
    aria-label="Profile JSON"
    rows="4"
    cols="80"
    bind:value={importText}
  ></textarea>
  <LayoutRenderer layout={tree} {source} {scope} />
</div>

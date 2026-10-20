<!-- S910 Settings screen: rendered 100% from ui/layouts/settings.default.json
      through LayoutRenderer + actions + bindings. The JSON owns the
      composition (centred 1100px grid, four section panels, descriptor-driven
      row repeats); row view-models flow in through `bind` expressions; row
      clicks dispatch ui.setSetting (screens/settings/actions.ts) through the
      store. This file only registers the layout: scope + profile + action binding. -->
<script lang="ts">
  import { live } from '../../state/live.svelte.ts';
  import { ui } from '../../state/uiStore.svelte.ts';
  import LayoutRenderer from '../../modules/LayoutRenderer.svelte';
  import { fixtureSource } from '../../modules/context.ts';
  import { createProfileStore } from '../../modules/profile/store.svelte.ts';
  import { bindUiActions } from '../../modules/actions.ts';
  import { useDeviceClass } from '../../modules/device.svelte.ts';
  import './actions.ts';
  import { buildSettingsVm } from './settingsVm.ts';
  import type { LayoutNode } from '../../modules/schema.ts';
  import defaults from '../../layouts/settings.default.json';

  let resetDialog: HTMLDialogElement;
  let clearing = $state(false);
  let resetError = $state('');
  async function resetStorage(): Promise<void> {
    if (clearing) return;
    clearing = true;
    resetError = '';
    try {
      await ui.do.clearAppStorage();
    } catch (error) {
      resetError = error instanceof Error ? error.message : 'Could not clear app storage. Please retry.';
      clearing = false;
    }
  }

  const store = createProfileStore('settings');
  bindUiActions(ui.do, { profile: store });

  // Responsive device class (viewport width): the phone settings grid is a
  // single column; desktop renders the design-exact grid.
  const device = useDeviceClass();

  const vm = $derived(buildSettingsVm(ui.state));
  const rows = (title: string) => vm.find(s => s.title === title)?.rows ?? [];
  const tree = $derived(store.tree(defaults as unknown as LayoutNode));
  const scope = $derived({
    store: ui.state,
    lists: {
      displayRows: rows('Display').map(row => row.key === 'spp' && device.now !== 'desktop'
        ? { ...row, label: 'Strip layout', desc: 'Touch screens use swipeable strips sized for fingers. The 8 / 12 / 16 setting applies on desktop.', control: 'none', options: [] }
        : row),
      faderRows: rows('Faders'),
      safetyRows: rows('Safety'),
      aboutRows: rows('About').map(row => row.key === 'console'
        ? { ...row, desc: live.connectionStatus(ui.state.connected === 'offline').label } : row),
    },
  });
  const source = fixtureSource({});
</script>

<LayoutRenderer layout={tree} {source} {scope} device={device.now} />

<section class="app-storage" aria-labelledby="storage-title">
  <div>
    <h2 id="storage-title">App storage</h2>
    <p>Remove saved preferences, offline mode, recent connection targets, custom layouts, user channel presets, and cached app data from this device. The app disconnects and returns to Connect with defaults. Mixer configuration is never changed.</p>
  </div>
  <button onclick={() => { resetError = ''; resetDialog.showModal(); }}>Clear app storage…</button>
</section>

<dialog bind:this={resetDialog} oncancel={(event) => { if (clearing) event.preventDefault(); }} aria-labelledby="reset-title" aria-describedby="reset-description">
  <h2 id="reset-title">Clear app storage?</h2>
  <p id="reset-description">This permanently removes this device’s saved preferences, offline flag, recent targets, custom layouts, user channel presets, and app caches. Any local session edits are discarded. The app will disconnect, reload, and open Connect with defaults.</p>
  <p>Your mixer configuration and data on an external relay are not changed. This cannot be undone.</p>
  {#if resetError}<p role="alert">{resetError}</p>{/if}
  <div class="reset-actions">
    <button disabled={clearing} onclick={() => resetDialog.close()}>Cancel</button>
    <button disabled={clearing} class="clear-storage" onclick={resetStorage}>{clearing ? 'Clearing…' : 'Clear app storage'}</button>
  </div>
</dialog>

<style>
  .app-storage { max-width:1100px; width:100%; box-sizing:border-box; margin:12px auto 0; padding:16px; display:flex; flex-wrap:wrap; align-items:center; gap:16px; background:var(--u4-panel-bg); border:1px solid var(--u4-line-panel); border-radius:3px; }
  .app-storage > div { flex:1; min-width:200px; }
  h2 { margin:0 0 8px; font-size:17px; color:var(--u4-text-1); }
  p { margin:0 0 12px; font-size:14px; line-height:1.5; color:var(--u4-text-5); }
  .app-storage p { margin:0; }
  button { min-height:44px; padding:10px 16px; border:1px solid var(--u4-line-ctl); border-radius:3px; background:var(--u4-btn-bg); color:var(--u4-text-1); font:inherit; cursor:pointer; }
  button:disabled { opacity:.6; cursor:wait; }
  dialog { box-sizing:border-box; width:min(480px, calc(100% - 32px)); max-height:calc(100% - 40px); overflow:auto; padding:24px; border:1px solid var(--u4-line-ctl); border-radius:6px; background:var(--u4-panel-bg); color:var(--u4-text-1); }
  dialog::backdrop { background:rgba(0,0,0,.7); }
  .reset-actions { display:flex; justify-content:flex-end; flex-wrap:wrap; gap:12px; margin-top:20px; }
  .clear-storage { border-color:#b9443e; background:#652b27; color:#fff; }
  [role="alert"] { color:#ffaaa4; }
</style>

<!-- Keep the connection flow in the module/profile system so Connect still
     supports the app's layout editor, saved profiles and accessibility. -->
<script lang="ts">
  import { ui } from '../../state/uiStore.svelte.ts';
  import { live } from '../../state/live.svelte.ts';
  import { useDeviceClass } from '../../modules/device.svelte.ts';
  import LayoutRenderer from '../../modules/LayoutRenderer.svelte';
  import { fixtureSource } from '../../modules/context.ts';
  import { createProfileStore } from '../../modules/profile/store.svelte.ts';
  import { bindUiActions } from '../../modules/actions.ts';
  import { buildConnectionVm } from './connectionVm.ts';
  import type { LayoutNode } from '../../modules/schema.ts';
  import defaults from '../../layouts/connection.default.json';

  const store = createProfileStore('connection');
  bindUiActions(ui.do, { profile: store });
  const device = useDeviceClass();
  // Retain legacy binding scope for user-added connection modules.
  const vm = $derived.by(() => {
    void live.version;
    const w = live.engine;
    return buildConnectionVm(ui.state, { phase: live.phase, message: live.message,
      host: live.host, latencyMs: live.latencyMs, strips: w?.stripCount ?? 0,
      capsReady: w?.capsReady ?? false, link: w?.link ?? null, devices: w?.devices,
      scannedAt: w?.scannedAt ?? 0, scanning: live.scanning, scanError: live.scanError, driver: w?.driver ?? '' });
  });
  const tree = $derived(store.tree(defaults as unknown as LayoutNode));
  const scope = $derived({ store: ui.state, conn: vm, lists: { devices: vm.devices } });
  const source = fixtureSource({});
</script>

<LayoutRenderer layout={tree} {source} {scope} device={device.now} />

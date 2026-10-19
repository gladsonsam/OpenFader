<!-- L1 Connection screen: rendered 100% from
     ui/layouts/connection.default.json through LayoutRenderer + actions +
     bindings (status card, device rows, relay card, offline card). This file
     only registers the layout: scope + profile + action binding. -->
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

  // The relay card is a phone/tablet addition (desktop stays design-exact,
  // so the 1280x720 fixture baselines are untouched); the renderer gates it via
  // `when: { device: [...] }`, which needs the live device class here.
  const device = useDeviceClass();

  const vm = $derived(
    // Re-read on every engine ingest (devices arrive after caps in the
    // attach snapshot; without this the list stays empty until the next
    // phase change).
    ((): ReturnType<typeof buildConnectionVm> => {
      void live.version;
      return buildConnectionVm(ui.state, {
        phase: live.phase,
        message: live.message,
        host: live.host,
        latencyMs: live.latencyMs,
        strips: live.engine?.stripCount ?? 0,
        devices: live.engine ? live.engine.devices.map(d => ({ ...d })) : undefined,
        linkName: live.engine?.link?.name ?? null,
        scannedAt: live.engine?.scannedAt ?? 0,
        driver: live.engine?.driver ?? '',
        suggestion: live.suggestedHost(),
      });
    })(),
  );
  const tree = $derived(store.tree(defaults as unknown as LayoutNode));
  const scope = $derived({
    store: ui.state,
    conn: vm,
    relay: {
      host: live.host,
      phase: live.phase,
      message: live.message,
      latencyMs: live.latencyMs,
      strips: live.engine?.stripCount ?? 0,
      suggestion: live.suggestedHost(),
    },
    lists: { devices: vm.devices },
  });
  const source = fixtureSource({});
</script>

<LayoutRenderer layout={tree} {source} {scope} device={device.now} />

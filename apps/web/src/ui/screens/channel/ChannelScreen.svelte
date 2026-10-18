<!-- W5b Channel screen: rendered 100% from ui/layouts/channel.default.json
     through LayoutRenderer + actions + bindings (tab list + when-switched
     tab content + master strip). The JSON owns the composition; the tab
     rail is data (tab descriptor list repeated over channel-tab-button),
     the content area renders the selected tab's layout, and the master
     strip is composed from its parts so users can rebuild it from channel
     modules. This file only builds the scope: store state + view-models
     (channelScope) + live meter feeds, then binds ui.do. -->
<script lang="ts">
  import { ui } from '../../state/uiStore.svelte.ts';
  import LayoutRenderer from '../../modules/LayoutRenderer.svelte';
  import { fixtureSource } from '../../modules/context.ts';
  import { createProfileStore } from '../../modules/profile/store.svelte.ts';
  import { bindUiActions } from '../../modules/actions.ts';
  import { useDeviceClass } from '../../modules/device.svelte.ts';
  import './actions.ts';
  import { buildChannelScope } from './channelScope.ts';
  import type { MeterSource } from '../../primitives/signal/Meter.svelte';
  import type { LayoutNode } from '../../modules/schema.ts';
  import defaults from '../../layouts/channel.default.json';

  const store = createProfileStore('channel');
  bindUiActions(ui.do, { profile: store });

  // Responsive device class (viewport width): the phone layout stacks the
  // tab rail / content / master strip vertically; desktop renders the
  // design-exact row, so diff baselines are untouched.
  const device = useDeviceClass();

  const stripMeter = $derived<MeterSource>({
    getLevel: () => ui.meter?.levelOf(ui.sel.id) ?? -90,
    getPeak: () => ui.meter?.peakOf(ui.sel.id) ?? null,
    getClip: () => (ui.meter ? ui.meter.clipping(ui.sel.id, Date.now()) : false),
    showPeak: ui.state.peakHold,
  });
  const hbarMeter = $derived<MeterSource>({
    getLevel: () => ui.meter?.levelOf(ui.sel.id) ?? -90,
  });
  const levelDb = $derived(ui.meter?.levelOf(ui.sel.id) ?? -90);
  const dynGetLevel = $derived(
    ui.frozen ? null : (() => ui.meter?.levelOf(ui.sel.id) ?? -90),
  );

  // Live 80 ms meter loop (M1 pattern); frozen fixtures paint silence.
  $effect(() => {
    if (ui.frozen) return;
    ui.stepMeters(Date.now() / 1000, Date.now());
    const iv = setInterval(() => ui.stepMeters(Date.now() / 1000, Date.now()), 80);
    return () => clearInterval(iv);
  });

  const tree = $derived(store.tree(defaults as unknown as LayoutNode));
  const scope = $derived(
    buildChannelScope({
      state: ui.state,
      chans: ui.chans,
      fx: ui.fx,
      sel: ui.sel,
      frozen: ui.frozen,
      stripMeter,
      hbarMeter,
      levelDb,
      dynGetLevel,
    }),
  );
  const source = fixtureSource({});
</script>

<LayoutRenderer layout={tree} {source} {scope} device={device.now} />

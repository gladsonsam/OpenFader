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
  import { live } from '../../state/live.svelte.ts';
  import { bridgeStrip } from '../../state/engineBridge.ts';
  import type { LiveGroupsSource } from './groupsModel.ts';
  import LayoutRenderer from '../../modules/LayoutRenderer.svelte';
  import { fixtureSource } from '../../modules/context.ts';
  import { createProfileStore } from '../../modules/profile/store.svelte.ts';
  import { bindUiActions } from '../../modules/actions.ts';
  import { useDeviceClass } from '../../modules/device.svelte.ts';
  import './actions.ts';
  import { buildChannelScope } from './channelScope.ts';
  import { stripStateOf } from '../mixer/stripState.ts';
  import type { DeskGroups } from '../mixer/mixerVm.ts';
  import type { MeterSource } from '../../primitives/signal/Meter.svelte';
  import type { LayoutNode } from '../../modules/schema.ts';
  import defaults from '../../layouts/channel.default.json';

  const store = createProfileStore('channel');
  bindUiActions(ui.do, { profile: store });

  // Responsive device class (viewport width): the phone layout stacks the
  // tab rail / content / master strip vertically; desktop renders the
  // design-exact row, so fixture baselines are untouched.
  const device = useDeviceClass();

  const stripMeter = $derived<MeterSource>({
    getLevel: () => live.liveLevelDb(ui.sel.id) ?? ui.meter?.levelOf(ui.sel.id) ?? -90,
    getPeak: () => ui.meter?.peakOf(ui.sel.id) ?? null,
    getClip: () => (ui.meter ? ui.meter.clipping(ui.sel.id, Date.now()) : false),
    showPeak: ui.state.peakHold,
  });
  const hbarMeter = $derived<MeterSource>({
    getLevel: () => live.liveLevelDb(ui.sel.id) ?? ui.meter?.levelOf(ui.sel.id) ?? -90,
  });
  const levelDb = $derived(live.liveLevelDb(ui.sel.id) ?? ui.meter?.levelOf(ui.sel.id) ?? -90);
  const dynGetLevel = $derived(
    ui.frozen ? null : (() => live.liveLevelDb(ui.sel.id) ?? ui.meter?.levelOf(ui.sel.id) ?? -90),
  );

  // Live 80 ms meter loop (M1 pattern); frozen fixtures paint silence.
  $effect(() => {
    if (ui.frozen) return;
    ui.stepMeters(Date.now() / 1000, Date.now());
    const iv = setInterval(() => ui.stepMeters(Date.now() / 1000, Date.now()), 80);
    return () => clearInterval(iv);
  });

  const tree = $derived(store.tree(defaults as unknown as LayoutNode));

  // Live desk groups source for the selected channel (null offline: the
  // groups card hides and fixture mode is untouched). Membership/master
  // values are echo-confirmed only — null until the engine confirms them.
  function liveGroupsSource(): LiveGroupsSource | null {
    void live.version; // re-read on every engine ingest (control frames only)
    const w = live.engine;
    if (live.phase !== 'online' || !w || !w.capsReady) return null;
    const ch = bridgeStrip(w, ui.sel.id);
    if (ch === null) return null;
    return {
      engineCh: ch,
      driver: w.driver,
      muteGroups: w.groupCaps.muteGroups,
      subgroups: w.groupCaps.subgroups,
      assignMg: w.groupCaps.mgAssign,
      assignSg: w.groupCaps.subAssign,
      muteGroupAssigned: (g) => w.muteGroupAssigned(ch, g),
      subgroupAssigned: (g) => w.subgroupAssigned(ch, g),
      muteGroupMaster: (g) => w.muteGroupMaster(g),
      subgroupMasterMuted: (g) => w.subgroupMaster(g)?.muted ?? null,
      // `link` is a real per-strip leaf on sim; on xair stereo link is the
      // global /config/chlink switch (unsupported) — the card says so.
      linkSupported: w.supports('link', ch),
      link: () => {
        const v = w.param(ch, 'link');
        return typeof v === 'boolean' ? v : null;
      },
    };
  }

  // Glanceable status icons for the selected channel's master strip
  // (phantom, groups, link, invert, delay — echo-confirmed only, null
  // offline: the icons' `when` hides them and fixture mode is untouched).
  function selStateOf(): DeskGroups | null {
    void live.version; // re-read on every engine ingest (control frames only)
    const w = live.engine;
    if (live.phase !== 'online' || !w || !w.capsReady) return null;
    const ch = bridgeStrip(w, ui.sel.id);
    if (ch === null) return null;
    return stripStateOf(w, ch);
  }

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
      liveGroups: liveGroupsSource(),
    }),
  );
  const scopeWithState = $derived({ ...scope, selState: selStateOf() });
  const source = fixtureSource({});
</script>

<LayoutRenderer layout={tree} source={source} scope={scopeWithState} device={device.now} />

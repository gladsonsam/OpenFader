<!-- W5b Channel screen: rendered 100% from ui/layouts/channel.default.json
     through LayoutRenderer + actions + bindings (tab list + when-switched
     tab content + master strip). The JSON owns the composition; the tab
     rail is data (tab descriptor list repeated over channel-tab-button),
     the content area renders the selected tab's layout, and the left strip
     reuses the shared mixer strip (strip.default.json through strip-frame)
     fed by the mixer's cell view-model for the selected channel. This file
     only builds the scope: store state + view-models
     (channelScope) + live meter feeds, then binds ui.do. -->
<script lang="ts">
  import { ui } from '../../state/uiStore.svelte.ts';
  import { live } from '../../state/live.svelte.ts';
  import { bridgeStrip } from '../../state/engineBridge.ts';
  import type { LiveGroupsSource } from './groupsModel.ts';
  import type { LiveDeskSource } from './deskModel.ts';
  import type { LiveInsertSource } from './insertModel.ts';
  import type { DeskSendsOverlay } from './channelModel.ts';
  import LayoutRenderer from '../../modules/LayoutRenderer.svelte';
  import { fixtureSource } from '../../modules/context.ts';
  import { createProfileStore } from '../../modules/profile/store.svelte.ts';
  import { bindUiActions } from '../../modules/actions.ts';
  import { useDeviceClass } from '../../modules/device.svelte.ts';
  import './actions.ts';
  import '../mixer/actions.ts';
  import { buildChannelScope } from './channelScope.ts';
  import { stripStateOf } from '../mixer/stripState.ts';
  import { stripVm } from '../mixer/mixerVm.ts';
  import { sendDestName, tapOptionsForSlot } from '../../world/mapping/params.ts';
  import type { DeskGroups } from '../mixer/mixerVm.ts';
  import type { MeterSource } from '../../primitives/signal/Meter.svelte';
  import type { LayoutNode } from '../../modules/schema.ts';
  import defaults from '../../layouts/channel.default.json';
  import stripDefaults from '../../layouts/strip.default.json';

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

  // Live desk assign source for the selected channel (null offline: the
  // assign card hides and fixture mode is untouched). Membership/master/
  // param values are echo-confirmed only — null until the engine confirms
  // them.
  function liveGroupsSource(): LiveGroupsSource | null {
    void live.version; // re-read on every engine ingest (control frames only)
    const w = live.engine;
    if (live.phase !== 'online' || !w || !w.capsReady) return null;
    const ch = bridgeStrip(w, ui.sel.id);
    if (ch === null) return null;
    const grCh = ch;
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
      dcaGroups: 4,
      assignDca: w.supports('dca1_assign', ch),
      dcaAssigned: (g) => {
        const v = w.param(ch, `dca${g + 1}_assign`);
        return typeof v === 'boolean' ? v : null;
      },
      lrSupported: w.supports('lr_assign', ch),
      lrAssign: () => {
        const v = w.param(ch, 'lr_assign');
        return typeof v === 'boolean' ? v : null;
      },
      panSupported: w.supports('pan', ch),
      pan: () => {
        const v = w.param(ch, 'pan');
        return typeof v === 'number' ? v : null;
      },
      automixSupported: w.supports('automix_on', ch) || w.supports('automix_weight', ch),
      automixOn: () => {
        const v = w.param(ch, 'automix_on');
        return typeof v === 'boolean' ? v : null;
      },
      automixWeight: () => {
        const v = w.param(ch, 'automix_weight');
        return typeof v === 'number' ? v : null;
      },
      grComp: () => {
        if (grCh >= w.meters.size) return null;
        const v = w.meters.grComp[grCh];
        return typeof v === 'number' && Number.isFinite(v) ? v : null;
      },
      // `link` is a real per-strip leaf on sim; on xair stereo link is the
      // global /config/chlink switch (unsupported) — the card says so.
      linkSupported: w.supports('link', ch),
      link: () => {
        const v = w.param(ch, 'link');
        return typeof v === 'boolean' ? v : null;
      },
    };
  }

  // Live desk-config source for the selected channel (LR assign +
  // automix weight, both caps-gated). Null offline/unmapped/unsupported:
  // the card hides and fixture mode is untouched.
  function liveDeskSource(): LiveDeskSource | null {
    void live.version; // re-read on every engine ingest (control frames only)
    const w = live.engine;
    if (live.phase !== 'online' || !w || !w.capsReady) return null;
    const ch = bridgeStrip(w, ui.sel.id);
    if (ch === null) return null;
    const lr = w.supports('lr_assign', ch);
    const automix = w.supports('automix_weight', ch);
    if (!lr && !automix) return null;
    return {
      engineCh: ch,
      driver: w.driver,
      lrSupported: lr,
      lrAssign: () => {
        const v = w.param(ch, 'lr_assign');
        return typeof v === 'boolean' ? v : null;
      },
      automixSupported: automix,
      automixWeight: () => {
        const v = w.param(ch, 'automix_weight');
        return typeof v === 'number' ? v : null;
      },
    };
  }

  // Live desk send overlay (group-on + tap point per send slot, the desk's
  // own slot list and destination names, all caps-gated per slot). Null
  // offline, unmapped or on a desk without a sendN_* plane (DL32S addresses
  // aux/fx instead): the fixture rows render untouched.
  function deskSendsSource(): DeskSendsOverlay | null {
    void live.version; // re-read on every engine ingest (control frames only)
    const w = live.engine;
    if (live.phase !== 'online' || !w || !w.capsReady) return null;
    const ch = bridgeStrip(w, ui.sel.id);
    if (ch === null) return null;
    return {
      grpon: (n: number) => {
        const v = w.param(ch, `send${n}_grpon`);
        return typeof v === 'boolean' ? v : null;
      },
      grponSupported: (n: number) => w.supports(`send${n}_grpon`, ch),
      // Desk tap enum 0..5 (IN/PREEQ/POSTEQ/PRE/POST/GRP); unknown reads
      // null so the row shows the unknown look until the echo lands.
      tap: (n: number) => {
        const v = w.param(ch, `send${n}_tap`);
        return typeof v === 'number' ? v : null;
      },
      tapSupported: (n: number) => w.supports(`send${n}_tap`, ch),
      // Per-slot legal choices: bus slots take all six, FX slots refuse
      // GRP (desk-verified 2026-10-20) — the row cycles within this set.
      tapOptions: (n: number) => tapOptionsForSlot(w.driver, n),
      // XR18: slots 1-10 answer, 11-16 are silent on the desk (no leaf) and
      // no sendN_* plane yields null and keeps the fixture rows.
      sendSlots: () => {
        const slots: number[] = [];
        for (let n = 1; n <= 16; n++) if (w.supports(`send${n}_level`, ch)) slots.push(n);
        return slots.length > 0 ? slots : null;
      },
      destName: (n: number) => sendDestName(w.driver, n),
    };
  }

  // Live desk-insert source for the selected channel (bypass + select,
  // both caps-gated). Null offline/unmapped/unsupported: the card hides
  // and fixture mode is untouched.
  function liveInsertSource(): LiveInsertSource | null {
    void live.version; // re-read on every engine ingest (control frames only)
    const w = live.engine;
    if (live.phase !== 'online' || !w || !w.capsReady) return null;
    const ch = bridgeStrip(w, ui.sel.id);
    if (ch === null) return null;
    const on = w.supports('insert_on', ch);
    const sel = w.supports('insert_sel', ch);
    if (!on && !sel) return null;
    return {
      engineCh: ch,
      driver: w.driver,
      onSupported: on,
      insertOn: () => {
        const v = w.param(ch, 'insert_on');
        return typeof v === 'boolean' ? v : null;
      },
      selSupported: sel,
      insertSel: () => {
        const v = w.param(ch, 'insert_sel');
        return typeof v === 'number' ? v : null;
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
      liveDesk: liveDeskSource(),
      liveInsert: liveInsertSource(),
      deskSends: deskSendsSource(),
    }),
  );
  // The left strip is the SAME shared strip the main mixer renders
  // (strip.default.json through `strip-frame` — mute, full fader, name —
  // not the old bespoke channel-* mini-strip). The cell is the mixer's own
  // view-model for the selected channel, so mute/fader/solo/pan/name behave
  // exactly like the mixer; `ui.setStripFader` comes from the mixer's
  // actions (imported above for its side effect).
  const selCell = $derived(
    (() => {
      const vm = stripVm(ui.state, ui.chans, ui.sel.id);
      if (vm === null) return null;
      return { ...vm, desk: selStateOf(), meter: stripMeter };
    })(),
  );
  const anatomy = $derived(store.tree(stripDefaults as unknown as LayoutNode));
  const scopeWithState = $derived({ ...scope, selState: selStateOf(), cell: selCell, anatomy });
  const source = fixtureSource({});
</script>

<LayoutRenderer layout={tree} source={source} scope={scopeWithState} device={device.now} />

<!-- W5c Mixer screen: rendered 100% from ui/layouts/mixer.default.json
     through LayoutRenderer + actions + bindings. The JSON owns the
     composition (scroll row + strip grid + master strip); strip view-models
     (mixerVm, each with its imperative meter feed) flow in through the scope
     (`bankItems` for `over: 'bank:visible'`, `lists.emptySlots` for the
     trailing empty cells, `master`, `frozen`, `store`, `anatomy`); drags/taps dispatch
     `ui.*` actions through the store (lock/muteEnable rules apply there).
     This file only provides scope + profile + action binding, plus the one
     shared 80 ms meter loop (frozen in fixture freeze mode, deterministic).
     Hot path stays compiled: faders drag local-first inside the P1 Fader and
     meters draw imperatively from the shared meterSim getters (the renderer
     does no work per animation frame). -->
<script lang="ts">
  import { ui } from '../../state/uiStore.svelte.ts';
  import { live } from '../../state/live.svelte.ts';
  import { bridgeStrip } from '../../state/engineBridge.ts';
  import { dbToPos } from '../../fixtures/designData.ts';
  import BusPopup, { type BusRowVm } from './BusPopup.svelte';
  import LayoutRenderer from '../../modules/LayoutRenderer.svelte';
  import { fixtureSource } from '../../modules/context.ts';
  import { createProfileStore } from '../../modules/profile/store.svelte.ts';
  import { bindUiActions } from '../../modules/actions.ts';
  import { useDeviceClass } from '../../modules/device.svelte.ts';
  import './actions.ts';
  import { mixerCells, mixerMasterCell, phoneGridCols, SLOT_COUNT, tabletGridCols } from './mixerVm.ts';
  import type { DeskGroups } from './mixerVm.ts';
  import { mixerSlots } from './mixerVm.ts';
  import type { LayoutNode, RepeatScope } from '../../modules/schema.ts';
  import defaults from '../../layouts/mixer.default.json';
  import stripDefaults from '../../layouts/strip.default.json';

  const store = createProfileStore('mixer');
  bindUiActions(ui.do, { profile: store });

  // Responsive device class (viewport width): phone/tablet layouts apply
  // the JSON `variants` (snapping strip pager, narrower master); desktop
  // renders the design-exact tree, so fixture baselines are untouched.
  const device = useDeviceClass();

  // One shared 80 ms meter loop (fixtures freeze it; live ticks it).
  $effect(() => {
    ui.stepMeters(Date.now() / 1000, Date.now());
    const iv = setInterval(() => ui.stepMeters(Date.now() / 1000, Date.now()), 80);
    return () => clearInterval(iv);
  });

  // Live poll-priority hint: the visible bank's engine strips poll first
  // (fader/mute, then name/pan, then the rest — engine-side core-first
  // focus). Silent no-op offline (guarded on `online` + `capsReady`), so
  // fixture mode and its fixture baselines are untouched. Re-sends only when
  // the bank or link changes (EngineWorld dedups repeat lists).
  $effect(() => {
    const w = live.engine;
    if (live.phase !== 'online' || !w || !w.capsReady) return;
    const chs: number[] = [];
    for (const id of mixerSlots(ui.state.bank)) {
      if (!id) continue;
      const ch = bridgeStrip(w, id);
      if (ch !== null) chs.push(ch);
    }
    if (chs.length) w.setVisible(chs);
  });

  const frozen = $derived(ui.frozen && ui.meter !== null);
  // Live relay meters win over the fixture sim when a link is up (the
  // closure reads the engine arrays imperatively each animation frame, so
  // no reactive re-derivation is needed for drawing).
  const liveDb = (id: string): number | null => live.liveLevelDb(id);

  // Live desk groups/link overlay (names/colours arrive via the fixture
  // sync; membership + link have no fixture fields so they ride the scope
  // directly). Null offline: the badges' `when` hides them and fixture
  // mode is untouched. Membership/master values start from the attach
  // snapshot and update on every echo, so a strip with nothing
  // confirmed yet shows no badges — never faked.
  const deskOf = (id: string): DeskGroups | null => {
    void live.version; // re-read on every engine ingest (control frames only)
    const w = live.engine;
    if (live.phase !== 'online' || !w || !w.capsReady) return null;
    const ch = bridgeStrip(w, id);
    if (ch === null) return null;
    const caps = w.groupCaps;
    const mg: number[] = [];
    const mgMuted: Array<boolean | null> = [];
    for (let g = 0; g < caps.muteGroups; g++) {
      if (w.muteGroupAssigned(ch, g) === true) {
        mg.push(g);
        mgMuted.push(w.muteGroupMaster(g));
      }
    }
    const sg: number[] = [];
    const sgMuted: Array<boolean | null> = [];
    for (let g = 0; g < caps.subgroups; g++) {
      if (w.subgroupAssigned(ch, g) === true) {
        sg.push(g);
        sgMuted.push(w.subgroupMaster(g)?.muted ?? null);
      }
    }
    // `link` is a real per-strip leaf on sim (unsupported on xair, where
    // stereo link is the global /config/chlink switch): null hides the chip.
    let link: boolean | null = null;
    if (w.supports('link', ch)) {
      const lv = w.param(ch, 'link');
      if (typeof lv === 'boolean') link = lv;
    }
    if (mg.length === 0 && sg.length === 0 && link === null) return null;
    return { mg, sg, mgMuted, sgMuted, link };
  };

  const cells = $derived(mixerCells(ui.state, ui.chans, ui.meter, frozen, liveDb, deskOf));
  // Live availability filter (item 4): online, only strips the desk
  // topology actually has (bridgeStrip-mapped) render — no phantom
  // strips on desks smaller than the fixture (XR18: 16 in, not 48).
  // Desktop and phone share these cells, so both filter identically.
  // Offline/fixture renders everything (the design baseline).
  const liveCells = $derived(
    (() => {
      const w = live.engine;
      if (live.phase !== 'online' || !w || !w.capsReady) return cells;
      return cells.filter(c => bridgeStrip(w, c.id) !== null);
    })(),
  );
  const master = $derived(mixerMasterCell(ui.state, ui.chans, ui.meter, frozen, liveDb, deskOf));

  // Bus selector popup rows (item 7): Main LR + bus masters + FX returns
  // (the owner pattern). Fixture lists render offline; online, only
  // topology-mapped rows survive. Names ride the fixture sync (live desk
  // names flow in); meters are live levels (true silence when unknown).
  const busRows = $derived(
    (() => {
      void live.version;
      const w = live.engine;
      const onlineLive = live.phase === 'online' && !!w?.capsReady;
      const ids = ['mainlr', 'mix1', 'mix2', 'mix3', 'mix4', 'mix5', 'mix6', 'mix7', 'mix8', 'fx1', 'fx2', 'fx3', 'fx4'];
      const rows: BusRowVm[] = [];
      for (const id of ids) {
        const c = ui.chans[id];
        if (!c) continue;
        if (onlineLive && w && bridgeStrip(w, id) === null) continue;
        const lv = onlineLive
          ? (live.liveLevelDb(id) ?? -90)
          : frozen
            ? -90
            : (ui.meter?.levelOf(id) ?? -90);
        rows.push({
          id,
          name: c.name,
          sub: c.src,
          state: c.mute ? 'MUTED' : 'open',
          stateColor: c.mute ? '#d92d22' : '#7f8a9c',
          active: master !== null && id === master.id,
          meterPct: (dbToPos(lv) * 100).toFixed(1),
          meterAria: c.name + ' ' + (c.mute ? 'muted' : 'open'),
        });
      }
      return rows;
    })(),
  );
  const emptyCount = $derived(Math.max(0, SLOT_COUNT - liveCells.length));
  const tree = $derived(store.tree(defaults as unknown as LayoutNode));
  // The user-editable strip anatomy, derived through the same profile store
  // (user edits to strip parts apply); each repeat frame renders it with its
  // own cell scope.
  const anatomy = $derived(store.tree(stripDefaults as unknown as LayoutNode));
  const scope = $derived({
    store: ui.state,
    // `over: 'bank:visible'` reads scope.bankItems (strip view-models; the
    // schema types it as string[] but expandRepeat passes objects through
    // as props.item, like `list:` items).
    bankItems: liveCells as unknown as string[],
    // Empty-slot keys must be unique (expandRepeat keys instances by item):
    // distinct placeholder ids, one per trailing slot.
    lists: { emptySlots: Array.from({ length: emptyCount }, (_, i) => `empty-${i}`) },
    master,
    frozen,
    anatomy,
    // Responsive column templates for the phone/tablet grid variants
    // (`bind: { columns: 'phoneCols' }` in mixer.default.json). Providing
    // them on desktop too is harmless: no variant reads them there.
    phoneCols: phoneGridCols(),
    tabletCols: tabletGridCols(),
  } as unknown as RepeatScope);
  const settings = $derived({ gridCols: ui.gridCols });
  const source = fixtureSource({});
</script>

<LayoutRenderer layout={tree} {source} {scope} {settings} device={device.now} />
{#if ui.state.busPopup}
  <BusPopup rows={busRows} onPick={id => ui.do.selectBusMaster(id)} onClose={() => ui.do.closeBusPopup()} />
{/if}

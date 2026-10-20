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
  import { dbToPos, bankDefs } from '../../fixtures/designData.ts';
  import { stripStateOf } from './stripState.ts';
  import BusPopup, { type BusRowVm } from './BusPopup.svelte';
  import LayoutRenderer from '../../modules/LayoutRenderer.svelte';
  import { fixtureSource } from '../../modules/context.ts';
  import { createProfileStore } from '../../modules/profile/store.svelte.ts';
  import { bindUiActions } from '../../modules/actions.ts';
  import { useDeviceClass } from '../../modules/device.svelte.ts';
  import './actions.ts';
  import { mixerAllCells, mixerCells, mixerMasterCell, mixerSlots, phoneGridCols, SLOT_COUNT, stripMeterSource, stripVm, tabletGridCols } from './mixerVm.ts';
  import type { DeskGroups } from './mixerVm.ts';
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
  // Live desk status overlay (names/colours arrive via the fixture
  // sync; membership + link + preamp leaves ride the scope directly).
  // Null offline: the badges/icons hide and fixture mode is untouched.
  // Every leaf is echo-confirmed (stripStateOf: supports() + 'known'
  // status gates) — a strip with nothing confirmed yet shows nothing,
  // never faked.
  const deskOf = (id: string): DeskGroups | null => {
    void live.version; // re-read on every engine ingest (control frames only)
    const w = live.engine;
    if (live.phase !== 'online' || !w || !w.capsReady) return null;
    const ch = bridgeStrip(w, id);
    if (ch === null) return null;
    return stripStateOf(w, ch);
  };

  const cells = $derived(mixerCells(ui.state, ui.chans, ui.meter, frozen, liveDb, deskOf));
  // Live availability filter (item 4): online, only strips the desk
  // topology actually has (bridgeStrip-mapped) render — no phantom
  // strips on desks smaller than the fixture (XR18: 16 in, not 48).
  // Desktop and phone share these cells, so both filter identically.
  // Offline/fixture renders everything (the design baseline).
  // Default-bank extension (XR18 owner pattern): on the `ch1` bank the
  // live view appends the stereo aux input plus the four FX returns in
  // desk order (1–16, Aux, FxRet 1–4) — strips the fixture bank never
  // listed. Other banks render their own items only; offline the fixture
  // baseline (16 cells) is untouched.
  const liveCells = $derived(
    (() => {
      const w = live.engine;
      if (live.phase !== 'online' || !w || !w.capsReady) return cells;
      const kept = cells.filter(c => bridgeStrip(w, c.id) !== null);
      if (ui.state.bank !== 'ch1') return kept;
      const extra: typeof kept = [];
      for (const id of ['aux1', 'fx1', 'fx2', 'fx3', 'fx4']) {
        if (kept.some(c => c.id === id) || extra.some(c => c.id === id)) continue;
        if (bridgeStrip(w, id) === null) continue;
        const vm = stripVm(ui.state, ui.chans, id);
        if (!vm) continue;
        extra.push({ ...vm, desk: deskOf(id), meter: stripMeterSource(ui.chans, ui.meter, frozen, ui.state.peakHold, id, liveDb) });
      }
      return [...kept, ...extra];
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
  // All 48 inputs in order for the phone snap pager (no banks: the rail is
  // hidden on phones). Desk badges ride along like the bank cells. Online,
  // the same topology filter as the desktop grid applies (XR18: 16 in, not
  // 48) -- unmapped strips have no desk counterpart (writes no-op, meters
  // -90) and must not page as phantom strips.
  const allCells = $derived(
    (() => {
      const cells = mixerAllCells(ui.state, ui.chans, ui.meter, frozen, liveDb, deskOf);
      const w = live.engine;
      if (live.phase !== 'online' || !w || !w.capsReady) return cells;
      return cells.filter(c => bridgeStrip(w, c.id) !== null);
    })(),
  );
  const tree = $derived(store.tree(defaults as unknown as LayoutNode));
  // Phone layer rail (the shell rail stays desktop-only): bank descriptors
  // for `over: 'list:banks'`, with the same live availability filter as the
  // shell rail (XR18: banks with no strip on the desk hide). Offline/fixture
  // keeps every bank. Labels are compact: the rail is 52px wide.
  const liveBanks = $derived(
    (() => {
      const w = live.engine;
      const all = bankDefs();
      if (live.phase !== 'online' || !w || !w.capsReady) return all;
      void live.version;
      return all.filter(b => b.items.some(it => bridgeStrip(w, it.id) !== null));
    })(),
  );
  const RAIL_SHORT: Record<string, string> = {
    ch1: '1-16', ch17: '17-32', ch33: '33-48', fx: 'FX', mix: 'Mix', grp: 'Grp', mtx: 'Mtx', dca: 'DCA',
  };
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
    lists: {
      emptySlots: Array.from({ length: emptyCount }, (_, i) => `empty-${i}`),
      allBankItems: allCells as unknown as string[],
      banks: liveBanks.map(b => ({ key: b.key, label: RAIL_SHORT[b.key] ?? b.label, color: b.color, text: b.text })),
    },
    master,
    frozen,
    anatomy,
    // Responsive column templates for the phone/tablet grid variants
    // (`bind: { columns: 'phoneCols' }` in mixer.default.json). Providing
    // them on desktop too is harmless: no variant reads them there.
    phoneCols: phoneGridCols(),
    tabletCols: tabletGridCols(liveCells.length),
  } as unknown as RepeatScope);
  const settings = $derived({ gridCols: ui.gridCols });
  const source = fixtureSource({});
</script>

<LayoutRenderer layout={tree} {source} {scope} {settings} device={device.now} />
{#if ui.state.busPopup}
  <BusPopup rows={busRows} onPick={id => ui.do.selectBusMaster(id)} onClose={() => ui.do.closeBusPopup()} />
{/if}

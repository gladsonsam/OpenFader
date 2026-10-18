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
  import LayoutRenderer from '../../modules/LayoutRenderer.svelte';
  import { fixtureSource } from '../../modules/context.ts';
  import { createProfileStore } from '../../modules/profile/store.svelte.ts';
  import { bindUiActions } from '../../modules/actions.ts';
  import { useDeviceClass } from '../../modules/device.svelte.ts';
  import './actions.ts';
  import { mixerCells, mixerMasterCell, phoneGridCols, SLOT_COUNT, tabletGridCols } from './mixerVm.ts';
  import type { LayoutNode, RepeatScope } from '../../modules/schema.ts';
  import defaults from '../../layouts/mixer.default.json';
  import stripDefaults from '../../layouts/strip.default.json';

  const store = createProfileStore('mixer');
  bindUiActions(ui.do, { profile: store });

  // Responsive device class (viewport width): phone/tablet layouts apply
  // the JSON `variants` (snapping strip pager, narrower master); desktop
  // renders the design-exact tree, so diff baselines are untouched.
  const device = useDeviceClass();

  // One shared 80 ms meter loop (fixtures freeze it; live ticks it).
  $effect(() => {
    ui.stepMeters(Date.now() / 1000, Date.now());
    const iv = setInterval(() => ui.stepMeters(Date.now() / 1000, Date.now()), 80);
    return () => clearInterval(iv);
  });

  const frozen = $derived(ui.frozen && ui.meter !== null);
  // Live relay meters win over the fixture sim when a link is up (the
  // closure reads the engine arrays imperatively each animation frame, so
  // no reactive re-derivation is needed for drawing).
  const liveDb = (id: string): number | null => live.liveLevelDb(id);
  const cells = $derived(mixerCells(ui.state, ui.chans, ui.meter, frozen, liveDb));
  const master = $derived(mixerMasterCell(ui.state, ui.chans, ui.meter, frozen, liveDb));
  const emptyCount = $derived(Math.max(0, SLOT_COUNT - cells.length));
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
    bankItems: cells as unknown as string[],
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

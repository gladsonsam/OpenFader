<!-- W5b channel tab fallback: keeps the registerChannelTab registry working
     after the layout conversion. Renders the selected tab's default layout
     JSON (the same layouts ChannelScreen composes) through LayoutRenderer
     with the full channel scope. Prefer layout composition for new work:
     users build custom channel tabs from channel modules directly. -->
<script lang="ts">
  import { live } from '../../state/live.svelte.ts';
  import { ui } from '../../state/uiStore.svelte.ts';
  import LayoutRenderer from '../../modules/LayoutRenderer.svelte';
  import { fixtureSource } from '../../modules/context.ts';
  import { createProfileStore } from '../../modules/profile/store.svelte.ts';
  import { bindUiActions } from '../../modules/actions.ts';
  import './actions.ts';
  import { buildChannelScope } from './channelScope.ts';
  import type { MeterSource } from '../../primitives/signal/Meter.svelte';
  import type { LayoutNode } from '../../modules/schema.ts';
  import inputJson from '../../layouts/input.default.json';
  import sendsJson from '../../layouts/sends.default.json';
  import eqJson from '../../layouts/eq.default.json';
  import gateJson from '../../layouts/gate.default.json';
  import compJson from '../../layouts/comp.default.json';

  const store = createProfileStore('channel-tab');
  bindUiActions(ui.do, { profile: store });

  const trees: Record<string, LayoutNode> = {
    input: inputJson as unknown as LayoutNode,
    sends: sendsJson as unknown as LayoutNode,
    eq: eqJson as unknown as LayoutNode,
    gate: gateJson as unknown as LayoutNode,
    comp: compJson as unknown as LayoutNode,
  };

  const stripMeter = $derived<MeterSource>({
    getLevel: () => live.liveLevelDb(ui.sel.id) ?? ui.meter?.levelOf(ui.sel.id) ?? -90,
    getPeak: () => live.livePeakDb(ui.sel.id) ?? ui.meter?.peakOf(ui.sel.id) ?? null,
    getClip: () => live.liveClipOn(ui.sel.id) ?? (ui.meter ? ui.meter.clipping(ui.sel.id, Date.now()) : false),
    showPeak: ui.state.peakHold,
  });
  const hbarMeter = $derived<MeterSource>({
    getLevel: () => live.liveLevelDb(ui.sel.id) ?? ui.meter?.levelOf(ui.sel.id) ?? -90,
  });
  const levelDb = $derived(live.liveLevelDb(ui.sel.id) ?? ui.meter?.levelOf(ui.sel.id) ?? -90);
  const dynGetLevel = $derived(
    ui.frozen ? null : (() => live.liveLevelDb(ui.sel.id) ?? ui.meter?.levelOf(ui.sel.id) ?? -90),
  );

  const tree = $derived(store.tree(trees[ui.state.tab] ?? trees['input']));
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
      dynGetReduction: (kind) => live.liveReductionDb(`${kind}/${ui.sel.id}`),
    }),
  );
  const source = fixtureSource({ meter: ref => live.liveReductionDb(ref) });
</script>

<LayoutRenderer layout={tree} {source} {scope} />

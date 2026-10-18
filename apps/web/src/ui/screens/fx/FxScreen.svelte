<!-- F1 FX screen: rendered 100% from ui/layouts/fx.default.json through
      LayoutRenderer + actions + bindings. The JSON owns the composition (slot
      grid via repeat, one detail panel reading the selected slot); slot, type
      and knob view-models flow in through `bind` expressions; slot/type/
      toggle/tap/knob events dispatch the ui.* FX actions through the store.
      This file only provides the layout: scope + profile + action binding.
      Live link (item 6/9): slots/types/params/meters come from the engine
      library online (writes go through the store forwards); fixture slots
      render offline. X-Air serves 4 placeholder slots with no type tables,
      so type grid + knobs collapse and the toggle hides. -->
<script lang="ts">
  import { ui } from '../../state/uiStore.svelte.ts';
  import { live } from '../../state/live.svelte.ts';
  import type { FxSlot } from '../../fixtures/designData.ts';
  import type { FxSlotView } from '../../world/types.ts';
  import LayoutRenderer from '../../modules/LayoutRenderer.svelte';
  import { fixtureSource } from '../../modules/context.ts';
  import { createProfileStore } from '../../modules/profile/store.svelte.ts';
  import { bindUiActions } from '../../modules/actions.ts';
  import {
    FX_PARAM_DEFS,
    buildFxSlots,
    fxIsDelay,
    fxKnobList,
    fxTapLabel,
    fxTitle,
    fxToggleVm,
    fxTypeButtons,
  } from './fxModel.ts';
  import type { LayoutNode } from '../../modules/schema.ts';
  import defaults from '../../layouts/fx.default.json';

  const store = createProfileStore('fx');
  bindUiActions(ui.do, { profile: store });

  /** Live FX rack (null offline: fixture slots render instead). */
  function liveFx(): FxSlot[] | null {
    void live.version;
    const w = live.engine;
    if (live.phase !== 'online' || !w || !w.capsReady) return null;
    const rows = w.fxSlots;
    if (!rows.length) return null;
    return rows.map((s: FxSlotView) => {
      const defs = FX_PARAM_DEFS[s.type] ?? [];
      return {
        type: s.type,
        on: s.on && !s.bypass,
        p: defs.map(d => (typeof s.params[d.key] === 'number' ? s.params[d.key]! : 0.5)),
        bpm: null,
      };
    });
  }

  const sel = $derived(ui.state.fxSel);
  const online = $derived(live.phase === 'online' && !!live.engine?.capsReady);
  const fxList = $derived(liveFx() ?? ui.fx);
  const liveSel = $derived(Math.min(sel, Math.max(0, fxList.length - 1)));
  const slot = $derived(fxList[liveSel]);
  const frozen = $derived(ui.frozen);

  // Meters: live engine levels win online (true desk levels/silence),
  // fixture sim otherwise. Frozen fixtures stay silent.
  const levelOf = $derived(
    frozen ? null : (id: string) => live.liveLevelDb(id) ?? ui.meter?.levelOf(id) ?? -90,
  );
  // Slot count: live caps bound the grid (xair: 4); fixture catalogue offline.
  // Zero/unknown caps fall back to the rack length (stale relays may omit fx).
  const slotCount = $derived(
    online
      ? (live.engine?.fxCaps && live.engine.fxCaps.slots > 0 ? live.engine.fxCaps.slots : fxList.length)
      : null,
  );

  const slots = $derived(
    buildFxSlots(fxList, liveSel, levelOf, slotCount).map((vm, i) => {
      // Unknown desk state ("—" placeholders): "?" instead of a fake On.
      const src = fxList[i];
      return src && src.type === '—' ? { ...vm, state: '?', stateColor: '#7f8a9c' } : vm;
    }),
  );
  const fx = $derived(
    slot
      ? {
          title: fxTitle(liveSel, slot),
          // Unknown desk state (xair placeholders read "—") hides the
          // toggle honestly instead of showing a fake On.
          toggle: slot.type === '—' ? undefined : fxToggleVm(slot),
          isDelay: fxIsDelay(slot),
          tapLabel: fxTapLabel(slot),
          types: online
            ? (live.engine?.fxCaps?.types.map(t => ({ label: t.label, pressed: t.label === slot.type, bg: t.label === slot.type ? '#b28cff' : '#2b3038', fg: t.label === slot.type ? '#1a0f2e' : '#c9d1dc' })) ?? [])
            : fxTypeButtons(slot.type),
        }
      : { title: '', toggle: undefined, isDelay: false, tapLabel: '', types: [] },
  );
  // Knobs: live type tables filter the row online (xair has no tables —
  // the row collapses honestly); fixture catalogue offline. Unknown desk
  // types ("—" placeholders) yield no knobs instead of throwing.
  const knobCaps = $derived(
    online
      ? (live.engine?.fxCaps?.types.find(t => t.label === slot?.type || t.id === slot?.type)?.params.map(p => 'fx:param:' + p.id) ?? [])
      : null,
  );
  const knobs = $derived(slot && FX_PARAM_DEFS[slot.type] ? fxKnobList(slot.type, knobCaps) : []);
  // The fixture desk has an FX rack exactly when the world carries slots;
  // the fx-* modules declare requires:['fx'] so a desk without FX honestly
  // removes them (wave 3 binds real caps here).
  const caps = $derived((ui.fx?.length ?? 0) > 0 ? ['fx'] : []);
  const tree = $derived(store.tree(defaults as unknown as LayoutNode));
  const scope = $derived({
    store: ui.state,
    slot,
    knobs,
    fx,
    lists: { fxSlots: slots },
  });
  const source = fixtureSource({});
</script>

<LayoutRenderer layout={tree} {source} {scope} {caps} />

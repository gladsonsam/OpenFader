<!-- F1 FX screen: rendered 100% from ui/layouts/fx.default.json through
      LayoutRenderer + actions + bindings. The JSON owns the composition (slot
      grid via repeat, one detail panel reading the selected slot); slot, type
      and knob view-models flow in through `bind` expressions; slot/type/
      toggle/tap/knob events dispatch the ui.* FX actions through the store.
      This file only registers the layout: scope + profile + action binding.
      FX meters are silent in fixture freeze mode (the design reference masks
      the meter bars); live mode reads the shared meter sim. -->
<script lang="ts">
  import { ui } from '../../state/uiStore.svelte.ts';
  import LayoutRenderer from '../../modules/LayoutRenderer.svelte';
  import { fixtureSource } from '../../modules/context.ts';
  import { createProfileStore } from '../../modules/profile/store.svelte.ts';
  import { bindUiActions } from '../../modules/actions.ts';
  import {
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

  const sel = $derived(ui.state.fxSel);
  const slot = $derived(ui.fx[sel]);
  const frozen = $derived(ui.frozen);

  const slots = $derived(
    buildFxSlots(
      ui.fx,
      sel,
      frozen ? null : (id) => ui.meter?.levelOf(id) ?? -90,
    ),
  );
  const fx = $derived(
    slot
      ? {
          title: fxTitle(sel, slot),
          toggle: fxToggleVm(slot),
          isDelay: fxIsDelay(slot),
          tapLabel: fxTapLabel(slot),
          types: fxTypeButtons(slot.type),
        }
      : { title: '', toggle: undefined, isDelay: false, tapLabel: '', types: [] },
  );
  const knobs = $derived(slot ? fxKnobList(slot.type, null) : []);
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

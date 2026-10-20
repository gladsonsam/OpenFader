<!-- F1 FX screen: rendered 100% from ui/layouts/fx.default.json through
      LayoutRenderer + actions + bindings. The JSON owns the composition (slot
      grid via repeat, one detail panel reading the selected slot); slot, type
      and knob view-models flow in through `bind` expressions; slot/type/
      toggle/tap/knob events dispatch the ui.* FX actions through the store.
      This file only provides the layout: scope + profile + action binding.
      Live link (item 6/9): slots/types/params/meters come from the engine
      library online (writes go through the store forwards); fixture slots
      render offline. X-Air serves 4 desk-native slots with caps.fx type
      tables (full display names; type-select writes the stable
      short-code id) + parNN knobs live; the on/bypass toggle stays
      hidden there (no verified leaf — the engine rejects those writes). -->
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
    fxArtFor,
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

  /** Desk-native tap-tempo types (proto-xair tap targets): DLY/3TAP/4TAP
      plus the D/RV, D/CR, D/FL and MODD combos. Design Delay handled
      separately via fxIsDelay. */
  const XAIR_TAP_TYPES = ['DLY', '3TAP', '4TAP', 'D/RV', 'D/CR', 'D/FL', 'MODD'];

  /** Desk-native param key order per live slot (parNN ids from caps.fx,
      aligned with the p vector; empty for design/fixture slots). */
  function liveKeys(): string[][] {
    void live.version;
    const w = live.engine;
    if (live.phase !== 'online' || !w || !w.capsReady) return [];
    const capsTypes = w.fxCaps?.types ?? [];
    return w.fxSlots.map((s: FxSlotView) => {
      const capsDef = capsTypes.find(t => t.id === s.type || t.label === s.type);
      return capsDef ? capsDef.params.map(p => p.id) : [];
    });
  }

  /** Live FX rack (null offline: fixture slots render instead). Desk-native
      racks (xair) resolve knob order from caps.fx type tables (parNN ids);
      design racks (sim) use the fixture catalogue keyed by param id. */
  function liveFx(): FxSlot[] | null {
    void live.version;
    const w = live.engine;
    if (live.phase !== 'online' || !w || !w.capsReady) return null;
    const rows = w.fxSlots;
    if (!rows.length) return null;
    const capsTypes = w.fxCaps?.types ?? [];
    return rows.map((s: FxSlotView) => {
      const capsDef = capsTypes.find(t => t.id === s.type || t.label === s.type);
      if (capsDef) {
        const keys = capsDef.params.map(p => p.id);
        const p = keys.map(k => (typeof s.params[k] === 'number' ? s.params[k]! : 0.5));
        return { type: s.type, on: s.on && !s.bypass, p, bpm: null };
      }
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

  // Full display name for a slot type code (`caps.fx` label for the
  // desk short code; design/fixture names pass through). Slot `type`
  // stays the stable wire key everywhere else (pickType, tap types).
  function displayNameOf(code: string): string {
    void live.version;
    const w = live.engine;
    if (live.phase !== 'online' || !w || !w.capsReady) return code;
    return w.fxCaps?.types.find(t => t.id === code || t.label === code)?.label ?? code;
  }

  const slots = $derived(
    buildFxSlots(fxList, liveSel, levelOf, slotCount).map((vm, i) => {
      // Unknown desk state ("—" placeholders): "?" instead of a fake On.
      const src = fxList[i];
      if (src && src.type === '—') return { ...vm, state: '?', stateColor: '#7f8a9c' };
      if (!online || !isXair || !src || src.type === '—') return vm;
      // Desk-native slot: full name + code caption + family tile.
      const code = src.type;
      return { ...vm, type: displayNameOf(code), code, art: fxArtFor(code) };
    }),
  );
  const isXair = $derived(online && live.engine?.driver === 'xair');
  const keys = $derived(liveKeys());
  const fx = $derived(
    slot
      ? {
          title: fxTitle(liveSel, { ...slot, type: displayNameOf(slot.type) }),
          // Unknown desk state ("—") hides the toggle honestly instead of
          // a fake On; xair has no verified on/bypass leaf (the engine
          // rejects FxOn/FxBypass there) so its toggle stays hidden even
          // though types + params read/write live.
          toggle: slot.type === '—' || isXair ? undefined : fxToggleVm(slot),
          isDelay: isXair
            ? XAIR_TAP_TYPES.includes(slot.type)
            : fxIsDelay(slot),
          tapLabel: fxTapLabel(slot),
          // Type-select is engine-verified on xair (Numbered rack: full
          // display names from caps.fx labels, e.g. Hall Reverb, written
          // back as the stable short-code id, e.g. HALL) and on design
          // racks alike; fixture catalogue offline. Unknown desk types
          // ("—") yield no grid (honest placeholder, never a faked write).
          types: slot.type === '—'
            ? []
            : online
              ? (live.engine?.fxCaps?.types.map(t => ({ label: t.label, value: t.id, code: t.id === t.label ? '' : t.id, art: fxArtFor(t.id), pressed: t.id === slot.type || t.label === slot.type, bg: t.id === slot.type || t.label === slot.type ? '#b28cff' : '#2b3038', fg: t.id === slot.type || t.label === slot.type ? '#1a0f2e' : '#c9d1dc' })) ?? [])
              : fxTypeButtons(slot.type),
        }
      : { title: '', toggle: undefined, isDelay: false, tapLabel: '', types: [] },
  );
  // Knobs: desk-native racks (xair) build the row from caps.fx (labeled
  // parNN params); design racks filter the fixture catalogue by caps;
  // fixture catalogue offline. Unknown desk types ("—") yield no knobs.
  const knobCaps = $derived(
    online
      ? (live.engine?.fxCaps?.types.find(t => t.label === slot?.type || t.id === slot?.type)?.params.map(p => 'fx:param:' + p.id) ?? [])
      : null,
  );
  const knobs = $derived.by(() => {
    if (!slot) return [];
    if (isXair) {
      const def = live.engine?.fxCaps?.types.find(t => t.label === slot.type || t.id === slot.type);
      if (!def) return [];
      const rowKeys = keys[liveSel] ?? def.params.map(p => p.id);
      return def.params.map((p, i) => ({ key: rowKeys[i] ?? p.id, label: p.label, bind: 'fx:param:' + p.id }));
    }
    if (!FX_PARAM_DEFS[slot.type]) return [];
    return fxKnobList(slot.type, knobCaps);
  });
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

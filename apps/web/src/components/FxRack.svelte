<script lang="ts">
  // FxRack: 4 X-Air FX slot cards (numbered params, no effect-type guessing).
  //
  // Driver mapping (crates/proto-xair/src/fx.rs): slot N 1..4, param MM 01..64
  // at `/fx/N/par/MM`, values 0.0..=1.0 bit-exact (hardware-verified read of
  // `/fx/1/par/01`; see docs/xr18-baseline.md §6 row 5). UI field naming goes
  // through the EXISTING `param` command — no protocol change:
  //   backend.setParam(slot - 1, `fx${slot}_par${MM}`, value01)
  // e.g. slot 1 par 1 -> setParam(0, 'fx1_par01', 0.5). `ch` carries the
  // 0-based slot so a future relay can route without parsing; the field is
  // authoritative. A relay/shell without an fx route rejects the field and
  // the card disables itself (unsupported-gracefully, same rule as Detail).
  //
  // v1 surface: par 01..08 live per slot. Par 09..64 share the identical
  // address/field pattern but have no effect-type labels yet, so each card
  // renders them as one disabled TODO row (no guessing at semantics) — bump
  // KNOWN_PARS when labels land.
  import type { Backend } from '../lib/backend';

  interface Props {
    backend: Backend;
    say?: (t: string) => void;
  }

  let { backend, say }: Props = $props();

  const KNOWN_PARS = 8; // par 01..08 live; 09..64 wait for effect-type labels
  const PARAM_COUNT = 64; // cf. core fx::FX_PARAM_COUNT

  const tell = (t: string): void => {
    say?.(t);
  };
  const errMsg = (e: unknown): string => (e instanceof Error ? e.message : String(e));

  /** UI field for (slot, par), or null when the mapping is unknown. */
  function fieldFor(slot: number, par: number): string | null {
    if (!Number.isInteger(slot) || slot < 1 || slot > 4) return null;
    if (!Number.isInteger(par) || par < 1 || par > PARAM_COUNT) return null;
    return `fx${slot}_par${String(par).padStart(2, '0')}`;
  }

  /** OSC address the field drives (title hints only). */
  function addrFor(slot: number, par: number): string {
    return `/fx/${slot}/par/${String(par).padStart(2, '0')}`;
  }

  // Which desk slot each card edits (default: card i -> slot i+1).
  let cardSlot: number[] = $state([1, 2, 3, 4]);
  // Local slider positions (the backend never echoes FX state, so these are
  // authoritative only after a touch). Key `${slot}:${par}`.
  let vals: Record<string, number> = $state({});
  // Slots the backend rejected: reason string disables the card's sliders.
  let dead: Record<number, string> = $state({});

  const key = (slot: number, par: number): string => `${slot}:${par}`;
  const pos = (slot: number, par: number): number => vals[key(slot, par)] ?? 0;

  function pickSlot(card: number, slot: number): void {
    cardSlot[card] = slot;
  }

  async function push(card: number, par: number, v01: number): Promise<void> {
    const slot = cardSlot[card] ?? card + 1;
    const field = fieldFor(slot, par);
    if (field === null) return; // unknown mapping — slider renders disabled
    vals[key(slot, par)] = v01;
    try {
      await backend.setParam(slot - 1, field, v01);
    } catch (e) {
      dead[slot] = errMsg(e);
      tell(`rack FX${slot} ${field}: ${errMsg(e)} (unsupported on this backend)`);
    }
  }

  type SliderEvent = Event & { currentTarget: EventTarget & HTMLInputElement };
</script>

<div class="rack">
  <div class="hint">Positions are local until touched — the backend never echoes FX state. First move pushes <code>fxN_parMM</code> (0..1) via the <code>param</code> command.</div>
  {#each [0, 1, 2, 3] as card (card)}
    {@const slot = cardSlot[card] ?? card + 1}
    {@const why = dead[slot]}
    <section class="card" class:dead={why !== undefined} aria-label={`FX slot card ${card + 1}`}>
      <header>
        <b>FX{slot}</b>
        <label>slot
          <select value={slot} onchange={(e) => pickSlot(card, Number(e.currentTarget.value))} disabled={why !== undefined}>
            {#each [1, 2, 3, 4] as n (n)}<option value={n}>{n}</option>{/each}
          </select>
        </label>
      </header>
      {#if why !== undefined}
        <div class="deadnote" title={why}>offline — backend rejected FX ({why})</div>
      {/if}
      {#each Array.from({ length: KNOWN_PARS }, (_, i) => i + 1) as par (par)}
        {@const field = fieldFor(slot, par)}
        {@const fxId = `fx-${slot}-${par}`}
        <div class="prow">
          <label
            for={fxId}
            title={field === null ? 'TODO: unmapped FX param (no guessing)' : `${field} -> ${addrFor(slot, par)}`}
          >
            {field === null ? `P${String(par).padStart(2, '0')} TODO` : `P${String(par).padStart(2, '0')}`}
          </label>
          <input
            id={fxId}
            type="range"
            min="0"
            max="1000"
            step="1"
            value={Math.round(pos(slot, par) * 1000)}
            disabled={field === null || why !== undefined}
            title={field === null ? 'TODO: driver mapping unknown — disabled, no guessing' : `${field} -> ${addrFor(slot, par)}`}
            oninput={(e: SliderEvent) => void push(card, par, Number(e.currentTarget.value) / 1000)}
          />
          <output>{Math.round(pos(slot, par) * 100)}%</output>
        </div>
      {/each}
      <div class="prow more">
        <label for="fx-{slot}-rest" title="par 09..64 share the fxN_parMM pattern but have no effect-type labels yet — no guessing">par 09–{PARAM_COUNT} TODO</label>
        <input id="fx-{slot}-rest" type="range" min="0" max="1000" value="0" disabled title="TODO: effect-type labels unknown — same fxN_parMM pattern, not yet mapped (no guessing)" />
        <output>—</output>
      </div>
    </section>
  {/each}
</div>

<style>
  .rack {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: 10px;
    padding: 10px;
  }
  .hint {
    grid-column: 1 / -1;
    font-size: 12px;
    color: #9aa4b2;
  }
  .hint code {
    color: #e2e8f0;
    background: #1a2230;
    padding: 0 4px;
    border-radius: 3px;
  }
  .card {
    background: #0f141c;
    border: 1px solid #1e293b;
    border-radius: 8px;
    padding: 8px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .card.dead {
    opacity: 0.75;
    border-style: dashed;
  }
  .card header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 14px;
  }
  .card header b {
    font-size: 16px;
  }
  .card header label {
    font-size: 12px;
    color: #9aa4b2;
    display: flex;
    gap: 4px;
    align-items: center;
  }
  .deadnote {
    font-size: 11px;
    color: #f87171;
  }
  .prow {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
  }
  .prow label {
    width: 34px;
    color: #9aa4b2;
  }
  .prow input[type='range'] {
    flex: 1;
  }
  .prow output {
    min-width: 36px;
    text-align: right;
    color: #e2e8f0;
  }
  .prow.more {
    opacity: 0.6;
  }
  .prow.more label {
    width: auto;
    flex: 1;
  }
</style>

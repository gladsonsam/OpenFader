<script lang="ts">
  // Console surface: input↔bus send matrix. Taps jump to that send mix
  // (select input + sends-on-fader on the bus) and show the cached send dB.
  //
  // Read-only where routing is concerned: the backend exposes no routing
  // writes (fader/mute/param only), so the patch itself is fixed — marked
  // below. Levels ride the existing `param` auxN_level path via App.setSend.
  import { fmtSendDb, sendDb, type SendMap } from '../lib/sends';
  import type { Strip as StripData } from '../lib/backend';

  interface Props {
    inputs: StripData[];
    buses: StripData[];
    sends: SendMap;
    selected: number;
    onCell: (ch: number, bus: number) => void;
  }

  let { inputs, buses, sends, selected, onCell }: Props = $props();
</script>

<div class="routing" aria-label="routing matrix">
  <div class="rhint">patch fixed on this backend (no routing writes) — tap a cell to jump to that send mix</div>
  {#if !inputs.length || !buses.length}
    <div class="empty">{!buses.length ? 'no buses on this backend yet' : 'no inputs'}</div>
  {:else}
    <div class="rmatrix" role="grid" aria-label="input to bus send levels" style={`--nbus: ${buses.length}`}>
      <div class="rcorner" role="presentation"></div>
      {#each buses as b (b.ch)}
        <div class="rbus" role="columnheader" title={`bus ${b.name}`}>{b.name}</div>
      {/each}
      {#each inputs as s (s.ch)}
        <div class="rrow" role="rowheader" class:sel={s.ch === selected} title={`${s.name} ch${s.ch + 1}`}>{s.name}</div>
        {#each buses as b (b.ch)}
          {@const db = sendDb(sends, s.ch, b.ch)}
          <button
            role="gridcell"
            class="rcell"
            class:live={db > -119.5}
            class:sel={s.ch === selected}
            aria-label={`${s.name} to ${b.name}: ${fmtSendDb(db)} dB — tap to jump to this send mix`}
            title={`${s.name} → ${b.name}: ${fmtSendDb(db)} dB — tap jumps (select + sends-on-fader)`}
            onclick={() => onCell(s.ch, b.ch)}
          >{fmtSendDb(db)}</button>
        {/each}
      {/each}
    </div>
  {/if}
</div>

<style>
  .routing { display: flex; flex-direction: column; gap: 6px; min-width: 0; }
  .rhint { font-size: 11px; color: var(--dim); }
  .empty { color: var(--dim); padding: 12px; }
  .rmatrix {
    display: grid;
    gap: 2px;
    overflow: auto;
    align-content: start;
    grid-template-columns: 90px repeat(var(--nbus, 8), minmax(64px, 1fr));
  }
  .rcorner { min-height: 22px; }
  .rbus {
    font-size: 11px; font-weight: 800; text-align: center; color: var(--acc);
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }
  .rrow {
    font-size: 11px; font-weight: 700; color: var(--dim);
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
    align-self: center; padding-right: 4px;
  }
  .rrow.sel { color: var(--txt); }
  .rcell {
    min-height: 0; min-width: 0; padding: 5px 2px; font-size: 11px;
    font-variant-numeric: tabular-nums; border-radius: 6px;
  }
  .rcell.live { border-color: var(--grn); color: var(--grn); }
  .rcell.sel { outline: 1px solid var(--acc); }
</style>

<script lang="ts">
  import { fly } from 'svelte/transition';
  import Strip from './Strip.svelte';
  import { eqBandsFor, grFor } from '../lib/bands';
  import type { Strip as StripData } from '../lib/backend';
  import type { LayerDef } from '../lib/persist';

  export type PaneCell =
    | { kind: 'strip'; s: StripData; master?: boolean }
    | { kind: 'gap'; gid: string };

  interface Props {
    /** Pane label shown in the pane header (e.g. "A" / "FOH"). */
    label: string;
    layers: LayerDef[];
    layer: string;
    cells: PaneCell[];
    buses: StripData[];
    sofBus: number | null;
    sofColor: string;
    sofName: string;
    selected: number;
    solos: number[];
    clips: number[];
    mutesEnabled: boolean;
    fine: boolean;
    editLayout: boolean;
    gr: Record<number, { gate: number; comp: number }>;
    locks: Record<number, string>;
    myLocks: number[];
    locksSupported: boolean;
    displayFader: (s: StripData) => number;
    displayDb: (s: StripData) => string;
    onLayer: (id: string) => void;
    onFader: (s: StripData, v: number) => void;
    onMute: (s: StripData) => void;
    onSolo: (ch: number) => void;
    onSelect: (ch: number) => void;
    onValueTap: (s: StripData) => void;
    onMeterTap: (ch: number) => void;
    onMove: (idx: number, d: -1 | 1) => void;
    onGap: (idx: number) => void;
    onRemove: (idx: number) => void;
    onEnterSof: (busCh: number) => void;
    onExitSof: () => void;
    onCopyLr: () => void;
    onLockToggle: (ch: number) => void;
    registerMeter: (ch: number, cv: HTMLCanvasElement) => void;
    unregisterMeter: (ch: number) => void;
  }

  let {
    label,
    layers,
    layer,
    cells,
    buses,
    sofBus,
    sofColor,
    sofName,
    selected,
    solos,
    clips,
    mutesEnabled,
    fine,
    editLayout,
    gr,
    locks,
    myLocks,
    locksSupported,
    displayFader,
    displayDb,
    onLayer,
    onFader,
    onMute,
    onSolo,
    onSelect,
    onValueTap,
    onMeterTap,
    onMove,
    onGap,
    onRemove,
    onEnterSof,
    onExitSof,
    onCopyLr,
    onLockToggle,
    registerMeter,
    unregisterMeter,
  }: Props = $props();

  const inSof = $derived(sofBus !== null);
  const builtin = $derived(layers.find((l) => l.id === layer)?.builtin ?? false);
</script>

<section class="mpane" class:sof={inSof} style={inSof ? `--sof: ${sofColor}` : ''} aria-label={`mixer pane ${label}`}>
  <div class="mprow">
    <span class="mplabel" title="independent pane: own layer + sends-on-fader mix">{label}</span>
    <div class="mptabs" role="tablist" aria-label={`fader banks pane ${label}`}>
      {#each layers as l (l.id)}
        <button
          role="tab"
          data-layer={l.id}
          class:on={layer === l.id}
          aria-selected={layer === l.id}
          tabindex={layer === l.id ? 0 : -1}
          onclick={() => onLayer(l.id)}
        >{l.name}</button>
      {/each}
    </div>
  </div>

  {#if buses.length}
    <nav class="mpaux" aria-label={`mix destination pane ${label}`}>
      <button class:lr={sofBus === null} aria-pressed={sofBus === null} onclick={onExitSof} title="back to LR">LR</button>
      {#each buses as b (b.ch)}
        <button
          class:dest={sofBus === b.ch}
          aria-pressed={sofBus === b.ch}
          style={sofBus === b.ch ? `border-color: ${sofColor}; color: ${sofColor}` : ''}
          onclick={() => (sofBus === b.ch ? onExitSof() : onEnterSof(b.ch))}
          title={`sends-on-fader to ${b.name}`}
          aria-label={`pane ${label} sends-on-fader to ${b.name}`}
        >{b.name}</button>
      {/each}
    </nav>
  {/if}

  {#if inSof}
    <div class="mpsof" role="alert">
      <b>SENDS → {sofName}</b>
      <button onclick={onCopyLr} title="start from current LR balance">copy LR</button>
      <button class="exit" onclick={onExitSof} title="exit sends-on-fader">EXIT ✕</button>
    </div>
  {/if}

  <div class="mpstrips">
    {#if !cells.length}
      <div class="empty">empty bank — reset layout restores backend order</div>
    {:else}
      {#each cells as c, i (c.kind === 'gap' ? c.gid : `ch${c.kind === 'strip' ? c.s.ch : i}${c.kind === 'strip' && c.master ? 'M' : ''}`)}
        {#if c.kind === 'gap'}
          <div class="spacer" title="blank spacer"></div>
        {:else}
          <div in:fly={{ y: 14, duration: 160, delay: Math.min(i, 8) * 25 }} class:masterpin={c.master}>
            {#if c.master}
              <div class="pinlabel" style={`background: ${sofColor}`}>MASTER</div>
            {/if}
            <Strip
              strip={c.s}
              selected={c.s.ch === selected}
              dimmed={solos.length > 0 && !solos.includes(c.s.ch)}
              soloed={solos.includes(c.s.ch)}
              muteLocked={!mutesEnabled}
              fine={fine}
              displayValue={displayFader(c.s)}
              displayDb={displayDb(c.s)}
              sofColor={inSof && !c.master ? sofColor : null}
              clip={clips.includes(c.s.ch)}
              bands={c.s.ch >= 0 ? eqBandsFor(c.s.ch) : null}
              grDb={grFor(c.s.ch, gr)}
              editMode={editLayout && !inSof}
              canRemove={!builtin || c.kind === 'strip'}
              onMove={(d) => onMove(i, d)}
              onGap={() => onGap(i)}
              onRemove={() => onRemove(i)}
              lockHolder={locksSupported ? (locks[c.s.ch] ?? null) : null}
              lockMine={myLocks.includes(c.s.ch)}
              onLockToggle={locksSupported ? () => onLockToggle(c.s.ch) : undefined}
              {registerMeter}
              {unregisterMeter}
              onFader={(v) => onFader(c.s, v)}
              onMute={() => onMute(c.s)}
              onSolo={() => onSolo(c.s.ch)}
              onSelect={() => onSelect(c.s.ch)}
              onValueTap={() => onValueTap(c.s)}
              onMeterTap={() => onMeterTap(c.s.ch)}
            />
          </div>
        {/if}
      {/each}
    {/if}
  </div>
</section>

<style>
  .mpane {
    display: flex; flex-direction: column; min-width: 0; flex: 1;
    border: 1px solid var(--line); border-radius: 12px; overflow: hidden;
    background: var(--bg);
  }
  .mpane.sof {
    outline: 3px solid var(--sof);
    outline-offset: -3px;
    background: color-mix(in srgb, var(--sof) 7%, var(--bg));
  }
  .mprow { display: flex; gap: 8px; align-items: center; padding: 8px; border-bottom: 1px solid var(--line); }
  .mplabel {
    flex: 0 0 auto; font-weight: 800; color: var(--acc);
    border: 1px solid var(--line); border-radius: 8px; padding: 6px 10px;
    background: var(--panel);
  }
  .mptabs { display: flex; gap: 6px; overflow-x: auto; flex: 1; }
  .mptabs button { flex: 1; font-weight: 700; white-space: nowrap; }
  .mptabs button.on { background: #12395a; border-color: var(--acc); }
  .mpaux {
    display: flex; gap: 6px; padding: 6px 8px; overflow-x: auto;
    border-bottom: 1px solid var(--line); background: var(--panel); align-items: center;
  }
  .mpaux button { flex: 0 0 auto; font-weight: 700; }
  .mpaux button.lr { background: #123a1e; border-color: var(--grn); }
  .mpaux button.dest { box-shadow: 0 0 12px currentColor; font-weight: 800; }
  .mpsof {
    display: flex; gap: 10px; align-items: center; padding: 6px 8px;
    background: color-mix(in srgb, var(--sof) 22%, #000);
    border-bottom: 2px solid var(--sof); font-size: 14px; flex-wrap: wrap;
  }
  .mpsof b { color: var(--sof); font-size: 15px; }
  .mpsof button.exit { margin-left: auto; border-color: var(--sof); color: var(--sof); font-weight: 800; }
  .mpstrips { flex: 1; display: flex; gap: 8px; overflow-x: auto; padding: 12px 10px; min-height: 340px; }
</style>

<script lang="ts">
  // Console surface meter page: full meterbridge navigation + RTA/spectrum
  // for the selected channel. Reuses MeterBridge (tap=jump, long-press=solo)
  // and Spectrum (live bins when the relay sends them, levels fallback).
  import MeterBridge from './MeterBridge.svelte';
  import Spectrum from './Spectrum.svelte';
  import type { Strip } from '../lib/backend';

  interface Props {
    strips: Strip[];
    selected: number;
    solos: number[];
    clips: number[];
    tick: number;
    spectrumCh: number;
    spectrum: number[] | null;
    onNavigate: (ch: number) => void;
    onSolo: (ch: number) => void;
    onClearClip: (ch: number) => void;
  }

  let { strips, selected, solos, clips, tick, spectrumCh, spectrum, onNavigate, onSolo, onClearClip }: Props = $props();

  const selName = $derived(strips.find((s) => s.ch === selected)?.name ?? (selected < 0 ? 'MAIN' : `ch${selected + 1}`));
</script>

<div class="meters" aria-label="meters and spectrum">
  <MeterBridge {strips} {selected} {solos} {clips} {tick} onNavigate={onNavigate} onSolo={onSolo} onClearClip={onClearClip} />
  <div class="rta">
    <div class="rtahead"><b>RTA</b><span>{selName}</span></div>
    <Spectrum ch={spectrumCh} bins={spectrum} />
  </div>
</div>

<style>
  .meters { display: flex; flex-direction: column; gap: 8px; min-width: 0; }
  .rta { display: flex; flex-direction: column; gap: 4px; }
  .rtahead { display: flex; gap: 8px; align-items: baseline; font-size: 12px; }
  .rtahead b { text-transform: uppercase; letter-spacing: 0.6px; }
  .rtahead span { color: var(--dim); }
</style>

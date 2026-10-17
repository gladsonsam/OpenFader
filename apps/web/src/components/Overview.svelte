<script lang="ts">
  // Overview = whole-show health (Mackie pattern): every channel one row of
  // name / meter / clip / mute / solo / fader-position, tap row to jump.
  // Meters read the shared `levels` array; `tick` is a ~8Hz pulse from the
  // App rAF loop that retriggers row reads without full state churn.
  import { levels } from '../lib/levels';
  import { faderDb } from '../lib/format';
  import type { Strip } from '../lib/backend';

  interface Props {
    strips: Strip[];
    solos: number[];
    clips: number[];
    tick: number;
    selected: number;
    mutesEnabled: boolean;
    onJump: (ch: number) => void;
    onMute: (s: Strip) => void;
    onSolo: (ch: number) => void;
    onSof: (busCh: number) => void;
    isBus: (s: Strip) => boolean;
  }

  let { strips, solos, clips, tick, selected, mutesEnabled, onJump, onMute, onSolo, onSof, isBus }: Props = $props();

  const lvlOf = (ch: number): number => (tick >= 0 ? (levels[ch] ?? 0) : 0);
  const clsOf = (v: number): string => (v > 0.95 ? 'hot' : v > 0.75 ? 'warm' : 'ok');
</script>

<div id="oview" role="table" aria-label="channel overview">
  {#each strips as s (s.ch)}
    {@const lv = lvlOf(s.ch)}
    <div class="orow" class:sel={s.ch === selected} class:muted={s.muted} role="row">
      <button
        class="ojump"
        onclick={() => onJump(s.ch)}
        title="tap: jump to this channel in Mixer"
      >
        <b>{s.name}</b><small>ch{s.ch + 1}</small>
      </button>
      <div class="obar" title="live meter">
        <div class="ofill {clsOf(lv)}" style="width: {Math.round(lv * 100)}%"></div>
      </div>
      <div class="oclip" class:lit={clips.includes(s.ch)} title="clip latch">●</div>
      <div class="ofader" title="fader {faderDb(s.fader)}">
        <div class="ofpos" style="left: {Math.round(s.fader * 100)}%"></div>
      </div>
      <button
        class="omute"
        class:lit={s.muted}
        disabled={!mutesEnabled}
        onclick={() => onMute(s)}
        title={mutesEnabled ? 'mute' : 'mutes disabled (MUTE EN off)'}
      >M</button>
      <button
        class="osolo"
        class:lit={solos.includes(s.ch)}
        onclick={() => onSolo(s.ch)}
        title="solo (local monitor)"
      >S</button>
      {#if isBus(s)}
        <button class="osof" onclick={() => onSof(s.ch)} title="sends-on-fader to {s.name}">mix›</button>
      {/if}
    </div>
  {/each}
  {#if !strips.length}
    <div class="empty">no strips</div>
  {/if}
</div>

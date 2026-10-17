<script lang="ts">
  // MeterBridge-as-navigation: the meter bridge is a nav surface.
  // Tap a channel cell -> select + jump to Mixer (App.jumpTo). If that
  // channel has a latched clip, the same tap also clears it — preserving
  // the existing tap-meter-to-clear contract, not duplicating it.
  // Long-press (>=500ms) -> solo toggle (local monitor, no protocol).
  // Reads the shared `levels` array (written by the existing onMeters
  // subscription); `tick` is the ~8Hz pulse from App's rAF loop that
  // retriggers reads without state churn. No backend/protocol changes.
  import { levels } from '../lib/levels';
  import type { Strip } from '../lib/backend';

  interface Props {
    strips: Strip[];
    selected: number;
    solos: number[];
    clips: number[];
    tick: number;
    onNavigate: (ch: number) => void;
    onSolo: (ch: number) => void;
    onClearClip: (ch: number) => void;
  }

  let { strips, selected, solos, clips, tick, onNavigate, onSolo, onClearClip }: Props = $props();

  const LONG_MS = 500;

  let longTimer: number | null = null;
  let longFired = false;

  const lvlOf = (ch: number): number => (tick >= 0 ? (levels[ch] ?? 0) : 0);
  const clsOf = (v: number): string => (v > 0.95 ? 'hot' : v > 0.75 ? 'warm' : 'ok');

  function cancelPress(): void {
    if (longTimer !== null) {
      window.clearTimeout(longTimer);
      longTimer = null;
    }
  }

  function pressStart(ch: number): void {
    cancelPress();
    longFired = false;
    longTimer = window.setTimeout(() => {
      longFired = true;
      longTimer = null;
      onSolo(ch);
    }, LONG_MS);
  }

  function pressEnd(): void {
    if (longTimer !== null) {
      window.clearTimeout(longTimer);
      longTimer = null;
    }
  }

  function tap(ch: number): void {
    if (longFired) {
      longFired = false;
      return;
    }
    cancelPress();
    if (clips.includes(ch)) onClearClip(ch);
    onNavigate(ch);
  }

  function cellKey(e: KeyboardEvent, ch: number): void {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      tap(ch);
    } else if (e.key === 's' || e.key === 'S') {
      e.preventDefault();
      cancelPress();
      onSolo(ch);
    }
  }
</script>

<div id="mbridge" role="list" aria-label="meter bridge: tap a channel to jump to it, long-press to solo">
  {#each strips as s (s.ch)}
    {@const lv = lvlOf(s.ch)}
    {@const isClip = clips.includes(s.ch)}
    {@const isSolo = solos.includes(s.ch)}
    {@const isSel = s.ch === selected}
    <div class="mcell" class:sel={isSel} class:soloed={isSolo} class:muted={s.muted} role="listitem">
      <button
        class="mtap"
        class:clip={isClip}
        aria-label={`${s.name} level ${Math.round(lv * 100)} percent${isClip ? ', CLIP latched — tap clears and jumps to Mixer' : ''}${isSolo ? ', soloed' : ''} — tap jumps to Mixer, long-press solos`}
        title={isClip
          ? `CLIP latched — tap clears + jumps to Mixer (long-press solos ${s.name})`
          : `tap: jump to ${s.name} in Mixer (long-press: solo)`}
        onclick={() => tap(s.ch)}
        onpointerdown={() => pressStart(s.ch)}
        onpointerup={pressEnd}
        onpointerleave={pressEnd}
        onpointercancel={pressEnd}
        oncontextmenu={(e) => e.preventDefault()}
        onkeydown={(e) => cellKey(e, s.ch)}
      >
        <span class="mclip" class:lit={isClip} aria-hidden="true">{isClip ? '●' : '○'}</span>
        <span class="mbar" role="img" aria-hidden="true">
          <span class="mfill {clsOf(lv)}" style="height: {Math.round(Math.min(1, Math.max(0, lv)) * 100)}%"></span>
        </span>
        <span class="mname">{s.name}</span>
      </button>
    </div>
  {/each}
  {#if !strips.length}
    <div class="empty">no strips</div>
  {/if}
</div>

<style>
  #mbridge {
    display: flex; gap: 4px; padding: 6px 10px; overflow-x: auto;
    background: #000; border-bottom: 1px solid var(--line);
  }
  .mcell { flex: 0 0 56px; }
  .mcell.sel .mtap { border-color: var(--acc); box-shadow: 0 0 0 1px var(--acc); }
  .mcell.soloed .mtap { border-color: var(--yel); }
  .mcell.muted { opacity: 0.6; }
  .mtap {
    width: 100%; min-width: 0; min-height: 0; padding: 4px 2px 6px;
    display: flex; flex-direction: column; align-items: center; gap: 3px;
    background: var(--panel); cursor: pointer; touch-action: manipulation;
  }
  .mtap.clip { outline: 2px solid var(--red); }
  .mclip { font-size: 12px; line-height: 1; color: rgba(255, 45, 45, 0.25); }
  .mclip.lit { color: #ff2d2d; text-shadow: 0 0 8px #ff2d2d; }
  .mbar {
    display: block; width: 18px; height: 44px; background: #0a0c0e;
    border-radius: 3px; overflow: hidden; position: relative;
  }
  .mfill { position: absolute; left: 0; right: 0; bottom: 0; background: var(--grn); }
  .mfill.warm { background: var(--yel); }
  .mfill.hot { background: var(--red); }
  .mname { font-size: 10px; color: var(--dim); white-space: nowrap; overflow: hidden; max-width: 100%; }
</style>

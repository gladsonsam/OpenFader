<script lang="ts">
  // Fat Channel thumbnail: mini EQ curve (SVG, same math as EqGraph via
  // lib/eqCurve — no duplication) + gain-reduction bar.
  //
  // Mixer-view integration (for the Strip/App owner — this component is
  // drop-in, no backend changes):
  //   import FatStrip from './FatStrip.svelte';
  //   <FatStrip bands={eqBandsFor(ch)} grDb={grFor(ch)} />
  // `bands` is the channel's 4-band state (Detail owns the selected channel's
  // copy; per-strip copies in the mixer need a ch->bands store — TODO once the
  // relay echoes param state). `grDb` is dB of gain reduction (>= 0), or null
  // until the relay/engine sends real GR frames (see V2.md deferred).
  // Strip.svelte renders this with `compact` so the 92px mixer strip keeps
  // its layout (svg scales to strip width, same curve math).
  import { bandPoint, curvePath, type EqBand } from '../lib/eqCurve';

  interface Props {
    bands: EqBand[];
    /** Gain reduction dB (>= 0). Null = no GR data yet. */
    grDb?: number | null;
    label?: string;
    /** Compact thumbnail for mixer strips: full-width, scaled svg. */
    compact?: boolean;
  }

  let { bands, grDb = null, label = 'GR', compact = false }: Props = $props();

  const W = 120;
  const H = 52;
  const d = $derived(curvePath(bands, W, H));
  const pct = $derived(grDb === null ? 0 : Math.min(100, Math.max(0, (grDb / 20) * 100)));
</script>

<div class="fat" class:compact title={grDb === null ? 'no GR data yet — needs relay/engine frames (see V2.md deferred)' : `${label} ${grDb.toFixed(1)} dB`}>
  <svg viewBox={`0 0 ${W} ${H}`} width={W} height={H} aria-hidden="true">
    <line x1="0" y1={H / 2} x2={W} y2={H / 2} stroke="rgba(148,163,184,0.4)" stroke-width="1" />
    <path {d} fill="none" stroke="#38bdf8" stroke-width="1.5" />
    {#each bands as b, i (i)}
      {@const p = bandPoint(b, W, H)}
      <circle cx={p.x} cy={p.y} r="2.5" fill={['#f472b6', '#38bdf8', '#a3e635', '#facc15'][i % 4]} />
    {/each}
  </svg>
  <div class="gr">
    <span class="gl">{label}</span>
    <div class="gtrack" class:nodata={grDb === null}>
      <div class="gfill" style={`width: ${pct}%`}></div>
    </div>
    <span class="gv">{grDb === null ? '—' : `${grDb.toFixed(0)}`}</span>
  </div>
</div>

<style>
  .fat {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 3px 4px;
    background: #0b0f14;
    border-radius: 5px;
    width: 128px;
  }
  .fat svg { display: block; }
  /* Compact (mixer Strip): fill the 92px strip, scale the svg by aspect. */
  .fat.compact { width: 100%; padding: 2px 3px; }
  .fat.compact svg { width: 100%; height: auto; }
  .gr {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 10px;
    color: #9aa4b2;
  }
  .gtrack {
    flex: 1;
    height: 6px;
    background: #1a2230;
    border-radius: 3px;
    overflow: hidden;
  }
  .gtrack.nodata {
    background: repeating-linear-gradient(45deg, #1a2230, #1a2230 3px, #141b28 3px, #141b28 6px);
  }
  .gfill {
    height: 100%;
    background: linear-gradient(90deg, #4ade80, #facc15 60%, #f87171);
  }
  .gv { min-width: 14px; text-align: right; }
</style>

<script lang="ts">
  // Console surface master section (pinned right): LR master strip (reuses
  // Strip — same fader/meter contract as the mixer), mute-group row, and a
  // phones/monitor-dim block that is LOCAL ONLY (no backend op exists).
  import Strip from './Strip.svelte';
  import Knob from './Knob.svelte';
  import type { MuteGroup } from '../lib/persist';
  import type { Strip as StripData } from '../lib/backend';

  interface Props {
    master: StripData;
    displayValue: number;
    displayDb: string;
    soloed: boolean;
    clip: boolean;
    mutesEnabled: boolean;
    mgroups: MuteGroup[];
    onToggleGroup: (i: number) => void;
    registerMeter: (ch: number, cv: HTMLCanvasElement) => void;
    unregisterMeter: (ch: number) => void;
    onFader: (v: number) => void;
    onMute: () => void;
    onSolo: () => void;
    onSelect: () => void;
    onValueTap: () => void;
    onMeterTap: () => void;
  }

  let {
    master,
    displayValue,
    displayDb,
    soloed,
    clip,
    mutesEnabled,
    mgroups,
    onToggleGroup,
    registerMeter,
    unregisterMeter,
    onFader,
    onMute,
    onSolo,
    onSelect,
    onValueTap,
    onMeterTap,
  }: Props = $props();

  // Local monitor only: the backend has no phones/dim op, so these never
  // leave this device (marked in the UI).
  let phones: number = $state(0.7);
  let dim: boolean = $state(false);
</script>

<section class="mastercol" aria-label="master section">
  <h3>MASTER</h3>
  <div class="bigmaster">
    <Strip
      strip={master}
      selected={true}
      {soloed}
      muteLocked={!mutesEnabled}
      displayValue={displayValue}
      displayDb={displayDb}
      {clip}
      {registerMeter}
      {unregisterMeter}
      onFader={(v) => onFader(v)}
      onMute={onMute}
      onSolo={onSolo}
      onSelect={onSelect}
      onValueTap={onValueTap}
      onMeterTap={onMeterTap}
    />
  </div>
  <div class="mgroups" role="group" aria-label="mute groups">
    <span class="lbl">groups</span>
    {#each mgroups as g, i (g.name)}
      <button
        class="grp"
        class:has={g.members.length > 0}
        onclick={() => onToggleGroup(i)}
        title={g.members.length ? `toggle mute: ${g.members.length} ch (${g.name})` : `${g.name} empty — assign in Channel view`}
        aria-label={g.members.length ? `toggle mute group ${g.name}, ${g.members.length} channels` : `mute group ${g.name}, empty`}
      >{g.name}{g.members.length ? `·${g.members.length}` : ''}</button>
    {/each}
  </div>
  <div class="phones" aria-label="monitor phones and dim, local only">
    <span class="lbl">phones <em>local</em></span>
    <Knob
      label="level"
      value={phones}
      min={0}
      max={1}
      step={0.01}
      readout={`${Math.round(phones * 100)}%`}
      size="sm"
      onInput={(v) => { phones = v; }}
    />
    <button
      class:lit={dim}
      aria-pressed={dim}
      onclick={() => { dim = !dim; }}
      title="local monitor dim — this device only, no backend op"
      aria-label={dim ? 'undim local monitor' : 'dim local monitor'}
    >{dim ? 'DIM✓' : 'DIM'}</button>
  </div>
</section>

<style>
  .mastercol {
    display: flex; flex-direction: column; gap: 8px; align-items: stretch;
    background: var(--panel); border: 1px solid var(--line); border-radius: 12px;
    padding: 8px; min-width: 0;
  }
  .mastercol h3 { margin: 0; font-size: 12px; letter-spacing: 1px; color: var(--dim); text-align: center; }
  .bigmaster { display: flex; justify-content: center; }
  /* Bigger LR meter without touching Strip: CSS scale only. */
  .bigmaster :global(.meter) { height: 230px; width: 22px; }
  .mgroups { display: flex; flex-wrap: wrap; gap: 4px; align-items: center; justify-content: center; }
  .lbl { color: var(--dim); font-size: 11px; text-transform: uppercase; letter-spacing: 0.5px; }
  .grp.has { border-color: var(--yel); }
  .lit { outline: 2px solid var(--yel); }
  .phones {
    display: flex; gap: 8px; align-items: center; justify-content: center;
    border-top: 1px solid var(--line); padding-top: 8px;
  }
  .phones em { font-style: normal; color: var(--yel); }
  .phones button.lit { background: #4a2b00; border-color: var(--yel); color: var(--yel); }
</style>

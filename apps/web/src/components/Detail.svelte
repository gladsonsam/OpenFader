<script lang="ts">
  import { SECTIONS, logToT, logFromT } from '../lib/sections';
  import type { FieldDef } from '../lib/sections';
  import type { MainStrip, ParamValue, Strip } from '../lib/backend';
  import { EQ_DEFAULTS, type EqBand } from '../lib/eqCurve';
  import Spectrum from './Spectrum.svelte';
  import EqGraph from './EqGraph.svelte';
  import FatStrip from './FatStrip.svelte';

  interface Props {
    // strip: selected strip object (ch >= 0) or MAIN marker (ch < 0) / null.
    strip: Strip | MainStrip | null;
    // hidden: param fields the backend rejected — those stay hidden.
    hidden?: string[];
    // spectrum: live bins for this channel (null until the relay sends them).
    spectrum?: number[] | null;
    // grGate/grComp: live gain reduction in dB (>= 0, 0 = no reduction).
    // Null = no GR source yet — the bars render the static scale until the
    // relay/engine sends real pre/post + GR frames (see V2.md deferred).
    // The parent passes these down like `spectrum` once a GR feed exists
    // (the xair driver exposes gate_gr/comp_gr via the param path; relay
    // plumbing is a follow-up — App does not wire them yet).
    grGate?: number | null;
    grComp?: number | null;
    onParam: (field: string, v: ParamValue) => void;
    onName: (v: string) => void;
  }

  // strip: selected strip object (ch >= 0) or null for MAIN.
  // hidden: array of param fields the backend rejected — those stay hidden.
  // spectrum: live bins for this channel (null until the relay sends them).
  // grGate/grComp: live GR dB when the parent has a feed, else null (static).
  let { strip, hidden = [], spectrum = null, grGate = null, grComp = null, onParam, onName }: Props = $props();

  // Local UI state resets whenever the selection changes
  // (parent wraps us in {#key strip?.ch}).
  let toggles: Record<string, boolean> = $state({});
  let labels: Record<string, string> = $state({});
  let name: string = $state('');
  let nameCh: number | null = $state(null);
  $effect(() => {
    // Seed the name field once per selection (track ch only, so backend
    // state pushes can't clobber an in-progress edit).
    const c = strip?.ch ?? null;
    if (c !== nameCh) {
      nameCh = c;
      name = strip?.name ?? '';
    }
  });

  // EQ band working state for EqGraph/FatStrip (engineering-unit defaults, same
  // as the slider defs). The backend never echoes param state, so this resets
  // per channel — exact values still land via onParam like the sliders.
  let eqBands: EqBand[] = $state(EQ_DEFAULTS.map((b) => ({ ...b })));
  let eqCh = $state(-999);
  $effect(() => {
    const c = strip?.ch ?? -999;
    if (c !== eqCh) {
      eqCh = c;
      eqBands = EQ_DEFAULTS.map((b) => ({ ...b }));
    }
  });

  // Gain reduction in dB (>= 0) arrives via the grGate/grComp props (live
  // when the parent has a GR feed, null until then — meters carry levels
  // only, so null renders a static scale until serve/relay sends real
  // pre/post + GR frames (see V2.md deferred).

  type SliderEvent = Event & { currentTarget: EventTarget & HTMLInputElement };

  function sliderInit(p: FieldDef): number {
    const def = p.def ?? 0;
    return p.type === 'logf' ? Math.round(logToT(def, p.min ?? 20, p.max ?? 20000) * 1000) : def;
  }

  function sliderLabel(p: FieldDef): string {
    const def = p.def ?? 0;
    return labels[p.f] ?? `${def}${p.unit ?? ''}`;
  }

  function onSlider(p: FieldDef, e: SliderEvent): void {
    const t = Number(e.currentTarget.value);
    const v = p.type === 'logf' ? Math.round(logFromT(t / 1000, p.min ?? 20, p.max ?? 20000)) : t;
    labels[p.f] = `${v}${p.unit ?? ''}`;
    onParam(p.f, v);
  }

  function onToggle(p: FieldDef): void {
    const next = !toggles[p.f];
    toggles[p.f] = next;
    onParam(p.f, next);
  }

  type SelectEvent = Event & { currentTarget: EventTarget & HTMLSelectElement };

  // `eqN_type` select: copy into eqBands[n].type so EqGraph/FatStrip render
  // shelves immediately. The backend still gets the exact number via onParam.
  function onEnum(p: FieldDef, e: SelectEvent): void {
    const v = Number(e.currentTarget.value);
    const m = /^eq([1-4])_type$/.exec(p.f);
    if (m) {
      const b = eqBands[Number(m[1]) - 1];
      if (b) b.type = v;
    }
    onParam(p.f, v);
  }
</script>

{#snippet grbar(label: string, v: number | null)}
  <div
    class="grrow"
    role="img"
    aria-label={v === null ? `${label}: no GR data yet` : `${label} reduction ${v.toFixed(1)} dB`}
    title={v === null
      ? 'no GR data yet — needs real pre/post + GR frames from serve/relay (V2.md deferred)'
      : `${label} reduction ${v.toFixed(1)} dB`}
  >
    <span class="gl">{label}</span>
    <div class="gtrack" class:nodata={v === null}>
      <div class="gfill" style={`width: ${v === null ? 0 : Math.min(100, (v / 24) * 100)}%`}></div>
    </div>
    <span class="gv">{v === null ? '—' : `${v.toFixed(1)}`}</span>
  </div>
{/snippet}

{#if !strip || strip.ch < 0}
  <h2>MAIN</h2>
  <div class="hint">Local monitor-dim in v1 (backend exposes no main fader yet). Use the console LR fader.</div>
{:else}
  <h2>{strip.name} <small>ch{strip.ch + 1}</small></h2>
  <Spectrum ch={strip.ch} bins={spectrum} />
  <h3>EQ</h3>
  <EqGraph ch={strip.ch} bands={eqBands} spectrum={spectrum} hidden={hidden} onParam={onParam} />
  <h3>Dynamics</h3>
  <div class="dyn">
    <FatStrip bands={eqBands} grDb={grComp} label="comp" />
    <div class="grcol">
      {@render grbar('gate', grGate)}
      {@render grbar('comp', grComp)}
      {#if grGate === null && grComp === null}
        <div class="hint">GR needs relay/engine frames — static scale until then (V2.md deferred).</div>
      {:else}
        <div class="hint">live GR (dB reduction)</div>
      {/if}
    </div>
  </div>
  <div class="prow">
    <label for="detail-name">name</label>
    <input
      id="detail-name"
      type="text"
      maxlength="16"
      value={name}
      onchange={(e) => { name = e.currentTarget.value; onName(name); }}
      style="flex: 1"
    />
  </div>
  {#each SECTIONS as sec (sec.title)}
    {@const vis = sec.fields.filter((p) => !hidden.includes(p.f))}
    {#if vis.length}
      <h3>{sec.title}</h3>
      {#each vis as p (p.f)}
        {#if p.type === 'b'}
          <div class="prow">
            <span class="plabel" id="detail-{p.f}">{p.label}</span>
            <button
              class="tgl"
              class:lit={!!toggles[p.f]}
              aria-labelledby="detail-{p.f}"
              aria-pressed={!!toggles[p.f]}
              onclick={() => onToggle(p)}
            >
              {toggles[p.f] ? 'on' : 'off'}
            </button>
          </div>
        {:else if p.type === 'enum'}
          <div class="prow">
            <label for="detail-{p.f}">{p.label}</label>
            <select id="detail-{p.f}" onchange={(e) => onEnum(p, e)}>
              {#each p.opts ?? [] as o (o)}<option value={o}>{o}</option>{/each}
            </select>
          </div>
        {:else}
          <div class="prow">
            <label for="detail-{p.f}">{p.label}</label>
            <input
              id="detail-{p.f}"
              type="range"
              min={p.type === 'logf' ? 0 : (p.min ?? 0)}
              max={p.type === 'logf' ? 1000 : (p.max ?? 100)}
              step={p.step ?? 1}
              value={sliderInit(p)}
              oninput={(e) => onSlider(p, e)}
            />
            <output>{sliderLabel(p)}</output>
          </div>
        {/if}
      {/each}
    {/if}
  {/each}
  <div class="hint">Controls the backend rejects disappear (unsupported on this mixer).</div>
{/if}

<style>
  .dyn {
    display: flex;
    gap: 10px;
    align-items: flex-start;
  }
  .grcol { flex: 1; display: flex; flex-direction: column; gap: 4px; }
  .grrow {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
  }
  .prow .plabel {
    flex: 0 0 86px;
    font-size: 13px;
    color: #9aa4b2;
  }
  .gl { width: 34px; color: #9aa4b2; }
  .gtrack {
    flex: 1;
    height: 8px;
    background: #1a2230;
    border-radius: 4px;
    overflow: hidden;
  }
  .gtrack.nodata {
    background: repeating-linear-gradient(45deg, #1a2230, #1a2230 4px, #141b28 4px, #141b28 8px);
  }
  .gfill {
    height: 100%;
    background: linear-gradient(90deg, #4ade80, #facc15 60%, #f87171);
  }
  .gv { min-width: 30px; text-align: right; color: #9aa4b2; }
</style>

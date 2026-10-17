<script>
  import { SECTIONS, logToT, logFromT } from '../lib/sections.js';

  // strip: selected strip object (ch >= 0) or null for MAIN.
  // hidden: array of param fields the backend rejected — those stay hidden.
  let { strip, hidden = [], onParam, onName } = $props();

  // Local UI state resets whenever the selection changes
  // (parent wraps us in {#key strip?.ch}).
  let toggles = $state({});
  let labels = $state({});
  let name = $state(strip?.name ?? '');

  function fieldDef(f) {
    return SECTIONS.flatMap((s) => s.fields).find((p) => p.f === f);
  }

  function sliderInit(p) {
    return p.type === 'logf' ? Math.round(logToT(p.def, p.min, p.max) * 1000) : p.def;
  }

  function sliderLabel(p) {
    return labels[p.f] ?? `${p.def}${p.unit ?? ''}`;
  }

  function onSlider(p, e) {
    const t = Number(e.currentTarget.value);
    const v = p.type === 'logf' ? Math.round(logFromT(t / 1000, p.min, p.max)) : t;
    labels[p.f] = `${v}${p.unit ?? ''}`;
    onParam(p.f, v);
  }

  function onToggle(p) {
    const next = !toggles[p.f];
    toggles[p.f] = next;
    onParam(p.f, next);
  }
</script>

{#if !strip || strip.ch < 0}
  <h2>MAIN</h2>
  <div class="hint">Local monitor-dim in v1 (backend exposes no main fader yet). Use the console LR fader.</div>
{:else}
  <h2>{strip.name} <small>ch{strip.ch + 1}</small></h2>
  <div class="prow">
    <label>name</label>
    <input
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
            <label>{p.label}</label>
            <button class="tgl" class:lit={!!toggles[p.f]} onclick={() => onToggle(p)}>
              {toggles[p.f] ? 'on' : 'off'}
            </button>
          </div>
        {:else if p.type === 'enum'}
          <div class="prow">
            <label>{p.label}</label>
            <select onchange={(e) => onParam(p.f, Number(e.currentTarget.value))}>
              {#each p.opts as o (o)}<option value={o}>{o}</option>{/each}
            </select>
          </div>
        {:else}
          <div class="prow">
            <label>{p.label}</label>
            <input
              type="range"
              min={p.type === 'logf' ? 0 : p.min}
              max={p.type === 'logf' ? 1000 : p.max}
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

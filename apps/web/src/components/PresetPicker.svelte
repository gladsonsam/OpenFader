<script lang="ts">
  import { SECTIONS } from '../lib/sections';
  import type { ParamValue } from '../lib/backend';

  // Factory strip starting points — web mirror of
  // crates/core/src/presets.rs (source of truth for values).
  // Keep the two in sync by hand. Values are conservative starting
  // points, not targets: the engineer still dials in.
  interface PresetSetting {
    field: string;
    value: ParamValue;
  }

  interface Preset {
    id: string;
    label: string;
    group: string;
    blurb: string;
    settings: PresetSetting[];
  }

  const F = (field: string, value: number): PresetSetting => ({ field, value });
  const B = (field: string, value: boolean): PresetSetting => ({ field, value });

  const PRESETS: Preset[] = [
    {
      id: 'kick', label: 'Kick', group: 'Drums',
      blurb: 'Low-cut 45 Hz, gate, 4:1, mud trim + beater click. Starting point.',
      settings: [B('hpf_on', true), F('hpf_freq', 45), B('gate_on', true), F('gate_threshold', -30), B('comp_on', true), F('comp_ratio', 4), F('comp_threshold', -18), B('eq_on', true), F('eq1_gain', -2), F('eq1_freq', 80), F('eq4_gain', 3), F('eq4_freq', 4000)],
    },
    {
      id: 'snare', label: 'Snare', group: 'Drums',
      blurb: 'Low-cut 90 Hz, gate, 3:1, box cut + snap. Starting point.',
      settings: [B('hpf_on', true), F('hpf_freq', 90), B('gate_on', true), F('gate_threshold', -28), B('comp_on', true), F('comp_ratio', 3), F('comp_threshold', -16), B('eq_on', true), F('eq2_gain', -3), F('eq2_freq', 450), F('eq4_gain', 2), F('eq4_freq', 6000)],
    },
    {
      id: 'hat', label: 'Hat', group: 'Drums',
      blurb: 'High low-cut, no gate, light 2:1, harshness trim. Starting point.',
      settings: [B('hpf_on', true), F('hpf_freq', 300), B('gate_on', false), B('comp_on', true), F('comp_ratio', 2), F('comp_threshold', -20), B('eq_on', true), F('eq1_gain', -3), F('eq1_freq', 250), F('eq4_gain', -2), F('eq4_freq', 10000)],
    },
    {
      id: 'tom', label: 'Tom', group: 'Drums',
      blurb: 'Low-cut 70 Hz, gate, 3:1, body + attack. Starting point.',
      settings: [B('hpf_on', true), F('hpf_freq', 70), B('gate_on', true), F('gate_threshold', -28), B('comp_on', true), F('comp_ratio', 3), F('comp_threshold', -16), B('eq_on', true), F('eq2_gain', 2), F('eq2_freq', 220), F('eq4_gain', 2), F('eq4_freq', 5000)],
    },
    {
      id: 'oh', label: 'OH', group: 'Drums',
      blurb: 'Low-cut 120 Hz, no gate, glue 2:1, low trim + air. Starting point.',
      settings: [B('hpf_on', true), F('hpf_freq', 120), B('gate_on', false), B('comp_on', true), F('comp_ratio', 2), F('comp_threshold', -22), B('eq_on', true), F('eq1_gain', -3), F('eq1_freq', 200), F('eq4_gain', 1.5), F('eq4_freq', 10000)],
    },
    {
      id: 'bass', label: 'Bass DI', group: 'Instruments',
      blurb: 'Sub-cut 35 Hz, no gate, 4:1, weight + mud cut. Starting point.',
      settings: [B('hpf_on', true), F('hpf_freq', 35), B('gate_on', false), B('comp_on', true), F('comp_ratio', 4), F('comp_threshold', -16), B('eq_on', true), F('eq1_gain', 2), F('eq1_freq', 90), F('eq2_gain', -2), F('eq2_freq', 350)],
    },
    {
      id: 'egtr', label: 'E-Gtr', group: 'Instruments',
      blurb: 'Low-cut 90 Hz, no gate, 3:1, rumble + fizz trim. Starting point.',
      settings: [B('hpf_on', true), F('hpf_freq', 90), B('gate_on', false), B('comp_on', true), F('comp_ratio', 3), F('comp_threshold', -18), B('eq_on', true), F('eq1_gain', -3), F('eq1_freq', 120), F('eq4_gain', -3), F('eq4_freq', 8000)],
    },
    {
      id: 'vox', label: 'Vox', group: 'Vocals',
      blurb: 'Low-cut 90 Hz, no gate, 3:1, low trim + presence + air. Starting point.',
      settings: [B('hpf_on', true), F('hpf_freq', 90), B('gate_on', false), B('comp_on', true), F('comp_ratio', 3), F('comp_threshold', -14), B('eq_on', true), F('eq1_gain', -2), F('eq1_freq', 200), F('eq2_gain', 2), F('eq2_freq', 3000), F('eq4_gain', 2), F('eq4_freq', 12000)],
    },
    {
      id: 'keys', label: 'Keys', group: 'Instruments',
      blurb: 'Low-cut 40 Hz, no gate, gentle 2:1, touch of presence. Starting point.',
      settings: [B('hpf_on', true), F('hpf_freq', 40), B('gate_on', false), B('comp_on', true), F('comp_ratio', 2), F('comp_threshold', -20), B('eq_on', true), F('eq2_gain', 1), F('eq2_freq', 2500)],
    },
    {
      id: 'playback', label: 'Playback', group: 'Playback',
      blurb: 'Protection low-cut only; dynamics off, EQ flat. Starting point.',
      settings: [B('hpf_on', true), F('hpf_freq', 40), B('gate_on', false), B('comp_on', false), B('eq_on', true), F('eq1_gain', 0), F('eq1_freq', 100), F('eq2_gain', 0), F('eq2_freq', 2000), F('eq3_gain', 0), F('eq3_freq', 3000), F('eq4_gain', 0), F('eq4_freq', 12000)],
    },
  ];

  // sections.ts `def` per field: the value a control shows before the
  // engineer touches it. Bools default off; enums default 0.
  const DEFAULTS = new Map<string, ParamValue>();
  for (const sec of SECTIONS) {
    for (const p of sec.fields) {
      if (p.type === 'b') DEFAULTS.set(p.f, false);
      else DEFAULTS.set(p.f, p.def ?? 0);
    }
  }
  const defaultFor = (field: string): ParamValue | undefined => DEFAULTS.get(field);

  interface Props {
    ch: number;
    hidden?: string[];
    onParam: (field: string, v: ParamValue) => void;
  }

  let { ch, hidden = [], onParam }: Props = $props();

  const groups: string[] = [...new Set(PRESETS.map((p) => p.group))];
  let selectedId: string | null = $state(null);
  // Single-level undo: the fields the last apply actually sent. Undo
  // returns those controls to their default positions — the backend
  // never echoes live param values, so a true snapshot-undo is not
  // possible here; capture a scene (Shows view) for a full snapshot.
  let lastApplied: { id: string; fields: string[] } | null = $state(null);
  let notice: string = $state('');

  const selected: Preset | null = $derived(PRESETS.find((p) => p.id === selectedId) ?? null);

  function fmt(v: ParamValue): string {
    return typeof v === 'boolean' ? (v ? 'on' : 'off') : `${v}`;
  }

  async function apply(p: Preset): Promise<void> {
    const sent: string[] = [];
    let skipped = 0;
    for (const s of p.settings) {
      if (hidden.includes(s.field)) {
        skipped += 1;
        continue;
      }
      await onParam(s.field, s.value);
      sent.push(s.field);
    }
    lastApplied = { id: p.id, fields: sent };
    notice =
      skipped > 0
        ? `${p.label} applied (${sent.length} set, ${skipped} unsupported here).`
        : `${p.label} applied (${sent.length} settings). Undo restores defaults.`;
  }

  async function undo(): Promise<void> {
    if (!lastApplied) return;
    const preset = PRESETS.find((p) => p.id === lastApplied!.id);
    let n = 0;
    for (const field of lastApplied.fields) {
      if (hidden.includes(field)) continue;
      const d = defaultFor(field);
      if (d === undefined) continue;
      await onParam(field, d);
      n += 1;
    }
    notice = preset ? `${preset.label} undone (${n} controls back to defaults).` : 'Undone.';
    lastApplied = null;
  }
</script>

<section class="presets" aria-label="factory strip presets">
  <h3>Presets <small>factory starting points — dial in by ear</small></h3>
  {#each groups as g (g)}
    <div class="pgroup">
      <span class="glabel">{g}</span>
      <div class="plist">
        {#each PRESETS.filter((p) => p.group === g) as p (p.id)}
          <button
            class="pbtn"
            class:sel={selectedId === p.id}
            aria-pressed={selectedId === p.id}
            onclick={() => { selectedId = selectedId === p.id ? null : p.id; notice = ''; }}
            title={p.blurb}
          >{p.label}</button>
        {/each}
      </div>
    </div>
  {/each}
  {#if selected}
    <div class="preview" aria-live="polite">
      <div class="phead">
        <strong>{selected.label}</strong>
        <span class="blurb">{selected.blurb}</span>
      </div>
      <ul class="psettings">
        {#each selected.settings as s (s.field)}
          <li class:skip={hidden.includes(s.field)}>
            <code>{s.field}</code>
            <span>{fmt(s.value)}{hidden.includes(s.field) ? ' (unsupported here)' : ''}</span>
          </li>
        {/each}
      </ul>
      <div class="pactions">
        <button class="apply" onclick={() => void apply(selected!)}>Apply to ch{ch + 1}</button>
        <button class="undo" disabled={!lastApplied} onclick={() => void undo()}>
          Undo{lastApplied ? ` (${PRESETS.find((p) => p.id === lastApplied!.id)?.label})` : ''}
        </button>
      </div>
    </div>
  {/if}
  {#if notice}
    <div class="notice" aria-live="polite">{notice}</div>
  {/if}
  {#if !selected && !notice}
    <div class="hint">Pick a preset to preview what it sets. Undo returns touched controls to defaults — capture a scene for a true snapshot.</div>
  {/if}
</section>

<style>
  .presets { margin-top: 8px; }
  .presets h3 small { font-weight: 400; color: #9aa4b2; }
  .pgroup { display: flex; align-items: baseline; gap: 8px; margin: 4px 0; }
  .glabel { flex: 0 0 86px; font-size: 12px; color: #9aa4b2; }
  .plist { display: flex; flex-wrap: wrap; gap: 4px; }
  .pbtn {
    padding: 3px 10px;
    border: 1px solid #2a3648;
    border-radius: 10px;
    background: #141b28;
    color: #dbe2ec;
    font-size: 12px;
    cursor: pointer;
  }
  .pbtn.sel { border-color: #4ade80; color: #4ade80; }
  .preview { margin-top: 6px; border: 1px solid #2a3648; border-radius: 6px; padding: 6px 8px; }
  .phead { display: flex; gap: 8px; align-items: baseline; }
  .blurb { font-size: 12px; color: #9aa4b2; }
  .psettings { list-style: none; margin: 6px 0; padding: 0; columns: 2; font-size: 12px; }
  .psettings li { display: flex; justify-content: space-between; gap: 8px; padding: 1px 0; break-inside: avoid; }
  .psettings li.skip { color: #6b7686; text-decoration: line-through; }
  .psettings code { color: #9aa4b2; }
  .pactions { display: flex; gap: 8px; }
  .apply, .undo {
    padding: 4px 14px;
    border-radius: 6px;
    border: 1px solid #4ade80;
    background: #1a2b1f;
    color: #4ade80;
    cursor: pointer;
  }
  .undo { border-color: #2a3648; background: #141b28; color: #dbe2ec; }
  .undo:disabled { opacity: 0.4; cursor: default; }
  .notice { margin-top: 6px; font-size: 12px; color: #4ade80; }
  .hint { margin-top: 6px; font-size: 12px; color: #9aa4b2; }
</style>

<!-- `strip-state-icons`: glanceable per-strip status icons directly beneath
     the mute button (mixer strips incl. master + channel-detail master).

     Display-only, echo-confirmed only: every glyph renders solely from
     engine-confirmed state (null/unsupported -> hidden, never faked).
     48V shows only when the desk confirms phantom on (amber); M1.. membership,
     S1.. membership, LINK, Ø (polarity invert) and D (delay on) are
     presence flags shown only when active (master-muted groups read red,
     matching the name-bar badge idiom at a smaller 14px size). Renders
     nothing — no box, no height — when the desk reports nothing for the
     strip, so fixture mode and unsupported desks are untouched. -->
<script lang="ts">
  import type { ModuleContext } from '../../modules/types.ts';
  import type { DeskGroups } from './mixerVm.ts';

  let {
    props = {},
    ctx: _ctx,
    nodeId: _nodeId,
  }: {
    props: { desk?: DeskGroups | null };
    ctx: ModuleContext;
    nodeId: string;
  } = $props();

  type Tone = 'amber' | 'red' | 'cyan' | 'dim' | 'bright';
  interface Glyph {
    key: string;
    label: string;
    title: string;
    tone: Tone;
  }

  const TONE_STYLE: Record<Tone, string> = {
    amber: 'border:1px solid var(--u4-amber-hi);color:var(--u4-amber-hi)',
    red: 'border:1px solid var(--u4-red-hi);color:var(--u4-red-hi)',
    cyan: 'border:1px solid var(--u4-cyan-hi);color:var(--u4-cyan-hi)',
    dim: 'border:1px solid var(--u4-line-ctl);color:var(--u4-text-5)',
    bright: 'border:1px solid var(--u4-line-ctl);color:var(--u4-text-1)',
  };

  const d = $derived(props.desk ?? null);
  const glyphs = $derived.by<Glyph[]>(() => {
    if (!d) return [];
    const out: Glyph[] = [];
    if (d.phantom === true) {
      out.push({
        key: 'phantom',
        label: '48V',
        tone: 'amber',
        title: 'Phantom power on (desk-confirmed)',
      });
    }
    d.mg.forEach((g, i) =>
      out.push({
        key: `mg${g}`,
        label: `M${g + 1}`,
        tone: d.mgMuted[i] ? 'red' : 'dim',
        title: `Mute group ${g + 1}${d.mgMuted[i] ? ' (master muted)' : ''} (desk-confirmed)`,
      }),
    );
    d.sg.forEach((g, i) =>
      out.push({
        key: `sg${g}`,
        label: `S${g + 1}`,
        tone: d.sgMuted[i] ? 'red' : 'dim',
        title: `Subgroup ${g + 1}${d.sgMuted[i] ? ' (master muted)' : ''} (desk-confirmed)`,
      }),
    );
    if (d.link === true) {
      out.push({ key: 'link', label: 'LINK', tone: 'cyan', title: 'Stereo linked (desk-confirmed)' });
    }
    if (d.polarity === true) {
      out.push({ key: 'pol', label: 'Ø', tone: 'bright', title: 'Polarity inverted (desk-confirmed)' });
    }
    if (d.delayOn === true) {
      out.push({ key: 'dly', label: 'D', tone: 'bright', title: 'Input delay on (desk-confirmed)' });
    }
    return out;
  });
  const label = $derived(() => {
    if (!d || glyphs.length === 0) return 'Strip status: nothing reported';
    const parts: string[] = [];
    if (d.phantom === true) parts.push('phantom on');
    d.mg.forEach((g, i) => parts.push(`mute group ${g + 1}${d.mgMuted[i] ? ', master muted' : ''}`));
    d.sg.forEach((g, i) => parts.push(`subgroup ${g + 1}${d.sgMuted[i] ? ', master muted' : ''}`));
    if (d.link === true) parts.push('stereo linked');
    if (d.polarity === true) parts.push('polarity inverted');
    if (d.delayOn === true) parts.push('delay on');
    return 'Strip status: ' + (parts.length ? parts.join('; ') : 'nothing reported');
  });
</script>

{#if glyphs.length > 0}
  <div
    role="group"
    aria-label={label()}
    style="box-sizing:content-box;flex:none;display:flex;flex-wrap:wrap;justify-content:center;gap:3px;padding:1px 0;min-width:0;min-height:16px"
  >
    {#each glyphs as g (g.key)}
      <span
        title={g.title}
        style="box-sizing:border-box;height:14px;display:inline-flex;align-items:center;padding:0 4px;border-radius:2px;background:var(--u4-bank-off);{TONE_STYLE[g.tone]};font-size:10px;font-weight:600;line-height:1"
        >{g.label}</span
      >
    {/each}
  </div>
{/if}

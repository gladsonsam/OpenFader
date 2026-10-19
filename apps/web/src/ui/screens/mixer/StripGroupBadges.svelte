<!-- `strip-group-badges`: live desk membership chips for one mixer strip.

     Display-only (membership toggles live on the channel groups card): one
     wrapped row of small chips under the name bar — `M1` per confirmed
     mute-group member, `S1` per confirmed subgroup member, `LINK` for the
     stereo-link leaf. Master-muted groups read red. Renders nothing without
     a live desk (`desk` null offline; LayoutRenderer `when` hides the node),
     so fixture mode and the fixture baselines are untouched. -->
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

  const d = $derived(props.desk ?? null);
  const show = $derived(
    !!d && (d.mg.length > 0 || d.sg.length > 0 || d.link !== null),
  );
  const label = $derived(() => {
    if (!d) return '';
    const parts: string[] = [];
    d.mg.forEach((g, i) => parts.push(`mute group ${g + 1}${d.mgMuted[i] ? ', master muted' : ''}`));
    d.sg.forEach((g, i) => parts.push(`subgroup ${g + 1}${d.sgMuted[i] ? ', master muted' : ''}`));
    if (d.link !== null) parts.push(d.link ? 'stereo linked' : 'stereo link off');
    return 'Desk groups: ' + (parts.length ? parts.join('; ') : 'none confirmed');
  });
</script>

{#if show && d}
  <div
    role="group"
    aria-label={label()}
    style="box-sizing:content-box;flex:none;display:flex;flex-wrap:wrap;justify-content:center;gap:2px;padding:2px 0;min-width:0"
  >
    {#each d.mg as g, i}
      <span
        title="Mute group {g + 1}{d.mgMuted[i] ? ' (master muted)' : ''}"
        style="box-sizing:border-box;height:18px;display:inline-flex;align-items:center;padding:0 5px;border-radius:3px;border:1px solid {d.mgMuted[i]
          ? 'var(--u4-red-hi)'
          : 'var(--u4-line-ctl)'};background:var(--u4-bank-off);color:{d.mgMuted[i] ? 'var(--u4-red-hi)' : 'var(--u4-text-5)'};font-size:11px;font-weight:600;line-height:1"
        >M{g + 1}</span
      >
    {/each}
    {#each d.sg as g, i}
      <span
        title="Subgroup {g + 1}{d.sgMuted[i] ? ' (master muted)' : ''}"
        style="box-sizing:border-box;height:18px;display:inline-flex;align-items:center;padding:0 5px;border-radius:3px;border:1px solid {d.sgMuted[i]
          ? 'var(--u4-red-hi)'
          : 'var(--u4-line-ctl)'};background:var(--u4-bank-off);color:{d.sgMuted[i] ? 'var(--u4-red-hi)' : 'var(--u4-text-5)'};font-size:11px;font-weight:600;line-height:1"
        >S{g + 1}</span
      >
    {/each}
    {#if d.link !== null}
      <span
        title={d.link ? 'Stereo linked' : 'Stereo link off'}
        style="box-sizing:border-box;height:18px;display:inline-flex;align-items:center;padding:0 5px;border-radius:3px;border:1px solid {d.link
          ? 'var(--u4-cyan-hi)'
          : 'var(--u4-line-ctl)'};background:var(--u4-bank-off);color:{d.link ? 'var(--u4-cyan-hi)' : 'var(--u4-text-5)'};font-size:11px;font-weight:600;line-height:1"
        >LINK</span
      >
    {/if}
  </div>
{/if}

<!-- `channel-groups-card`: desk mute-group / subgroup assignments for the
     selected channel, plus stereo-link state and group master mutes.

     Presentational: the GroupsVm object in via layout `bind`, membership /
     master commits out via `onEvent` (`assign` {kind, group, member},
     `master` {kind, group, muted}). Live-only: the layout `when` hides the
     card offline, so fixture mode and the fixture baselines are untouched.
     Unknown (not-yet-echoed) states render as `?` / dim — never faked. -->
<script lang="ts">
  import type { ModuleContext } from '../../modules/types.ts';
  import type { GroupsVm } from './groupsModel.ts';

  let {
    props = {},
    ctx: _ctx,
    nodeId: _nodeId,
    onEvent,
  }: {
    props: { vm?: GroupsVm | null };
    ctx: ModuleContext;
    nodeId: string;
    onEvent?: (name: string, payload?: unknown) => void;
  } = $props();

  const vm = $derived(props.vm ?? null);
  const linkText = $derived(
    !vm ? '' : vm.link === 'on' ? 'LINK' : vm.link === 'off' ? 'LINK OFF' : vm.link === 'unsupported' ? 'NO LINK LEAF' : 'LINK ?',
  );
  const linkTitle = $derived(
    !vm
      ? ''
      : vm.link === 'on'
        ? 'Stereo linked'
        : vm.link === 'off'
          ? 'Stereo link off'
          : vm.link === 'unsupported'
            ? 'Stereo link is a global desk switch (no per-channel leaf)'
            : 'Stereo link state unknown',
  );
</script>

{#if vm}
  <div
    style="box-sizing:content-box;flex:1 1 auto;min-height:120px;overflow-y:auto;display:flex;flex-direction:column;gap:6px;padding:10px;border-radius:3px;border:1px solid #1f2836;background:#0a0f17"
  >
    <div style="display:flex;flex-direction:column;align-items:stretch;gap:4px">
      <span style="font-size:15px;font-weight:600;color:#eef2f7;white-space:nowrap;overflow:hidden;text-overflow:ellipsis"
        >{vm.title}</span
      >
      <span
        title={linkTitle}
        style="box-sizing:border-box;height:22px;display:inline-flex;align-items:center;align-self:flex-start;padding:0 6px;border-radius:3px;border:1px solid {vm.link ===
        'on'
          ? '#5fe3e0'
          : '#3a414c'};background:#161d29;color:{vm.link === 'on' ? '#5fe3e0' : '#7f8a9c'};font-size:11px;font-weight:600"
        >{linkText}</span
      >
    </div>
    {#each [{ rows: vm.mg, kind: 'mg', heading: 'Mute groups' }, { rows: vm.sg, kind: 'sg', heading: 'Subgroups' }] as sec}
      {#if sec.rows.length > 0}
        <div style="font-size:12px;color:#7f8a9c">{sec.heading}</div>
        {#each sec.rows as r}
          <div style="display:flex;flex-direction:column;gap:4px">
            <span style="font-size:13px;font-weight:600;color:#eef2f7">{r.label}</span>
            <div style="display:flex;align-items:center;gap:8px">
              {#if r.assign}
                <button
                  onclick={() => onEvent?.('assign', { kind: sec.kind, group: r.group, member: !(r.member === true) })}
                  aria-pressed={r.member === true ? 'true' : 'false'}
                  aria-label={r.memberAria}
                  title={r.member === null ? 'Membership not confirmed yet — tap to set' : undefined}
                  style="box-sizing:border-box;font-variant-numeric:normal;width:56px;height:40px;flex:none;border-radius:3px;border:1px solid #3a414c;background:{r.memberBg};color:{r.memberFg};font-size:12px;font-weight:600;cursor:pointer"
                  >{r.memberLabel}</button
                >
              {/if}
              <button
                onclick={() => onEvent?.('master', { kind: sec.kind, group: r.group, muted: !(r.masterMuted === true) })}
                aria-pressed={r.masterMuted === true ? 'true' : 'false'}
                aria-label={r.masterAria}
                title={r.masterMuted === null ? 'Master state unknown — tap to set' : undefined}
                style="box-sizing:border-box;font-variant-numeric:normal;flex:1;min-width:0;height:40px;border-radius:3px;border:1px solid #3a414c;background:{r.masterBg};color:{r.masterFg};font-size:12px;font-weight:600;cursor:pointer"
                >{r.masterMuted === true ? 'MUTED' : r.masterMuted === false ? 'MUTE' : 'MUTE ?'}</button
              >
            </div>
          </div>
        {/each}
      {/if}
    {/each}
    {#if vm.note}
      <div style="font-size:12px;color:#7f8a9c">{vm.note}</div>
    {/if}
  </div>
{/if}

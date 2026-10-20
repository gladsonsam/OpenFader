<!-- `channel-groups-card`: the channel ASSIGN tab — LR assign, pan, DCA
     groups, mute groups (membership + masters) and automix (enable +
     weight + live gain-reduction) for the selected channel, plus
     stereo-link state.

     Presentational: the GroupsVm object in via layout `bind`, membership /
     param commits out via `onEvent` (`assign` {kind, group, member},
     `master` {kind, group, muted}, `dca` {group, member}, `lr`
     {assigned}, `pan` {value}, `automixOn` {on}, `automix` {value}).
     The GR readout animates from the `getGr` reader (150 ms poll, like
     the meter primitives); everything else is echo-confirmed vm data.
     Live-only: the layout `when` hides the card offline, so fixture mode
     and the fixture baselines are untouched. Unknown (not-yet-echoed)
     states render as `?` / dim — never faked. -->
<script lang="ts">
  import type { ModuleContext } from '../../modules/types.ts';
  import type { GroupsVm } from './groupsModel.ts';

  let {
    props = {},
    ctx: _ctx,
    nodeId: _nodeId,
    onEvent,
  }: {
    props: { vm?: GroupsVm | null; getGr?: (() => number | null) | null; showPan?: boolean };
    ctx: ModuleContext;
    nodeId: string;
    onEvent?: (name: string, payload?: unknown) => void;
  } = $props();

  const vm = $derived(props.vm ?? null);
  const getGr = $derived(props.getGr ?? null);
  // Live GR readout (dB reduction): polls the injected reader so the
  // meter moves with signal; falls back to the echo-confirmed vm value
  // (null = no meter data yet renders an honest dash).
  let grLive = $state<number | null>(null);
  $effect(() => {
    if (!getGr || !vm?.automixSupported) return;
    grLive = getGr();
    const iv = setInterval(() => {
      try {
        grLive = getGr();
      } catch {
        /* reader is best-effort; keep the last value */
      }
    }, 150);
    return () => clearInterval(iv);
  });
  const grText = $derived(
    grLive === null ? (vm?.grText ?? '—') : `${grLive.toFixed(1)} dB`,
  );
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
    style="box-sizing:content-box;flex:1 1 auto;min-height:120px;overflow-y:auto;display:flex;flex-direction:column;gap:10px;padding:10px;border-radius:3px;border:1px solid #1f2836;background:#0a0f17"
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
    {#if vm.lrSupported || (vm.panSupported && props.showPan !== false)}
      <div style="display:flex;flex-direction:column;gap:4px">
        {#if vm.lrSupported}
          <button
            onclick={() => onEvent?.('lr', { assigned: !(vm.lrAssigned === true) })}
            aria-pressed={vm.lrAssigned === true ? 'true' : 'false'}
            aria-label={vm.lrAria}
            title={vm.lrAssigned === null ? 'LR assign not confirmed yet — tap to set' : undefined}
            style="box-sizing:border-box;font-variant-numeric:normal;width:100%;height:40px;flex:none;border-radius:3px;border:1px solid #3a414c;background:{vm.lrBg};color:{vm.lrFg};font-size:12px;font-weight:600;cursor:pointer"
            >{vm.lrLabel}</button
          >
        {/if}
        {#if vm.panSupported && props.showPan !== false}
          <label style="display:flex;flex-direction:column;gap:4px">
            <span style="font-size:12px;color:#7f8a9c">Pan · {vm.panText}</span>
            <input
              type="range"
              min="-100"
              max="100"
              step="1"
              value={vm.pan === null ? 0 : Math.round(vm.pan)}
              aria-label={vm.panAria}
              title={vm.pan === null ? 'Pan not confirmed yet — drag to set' : undefined}
              onchange={e => onEvent?.('pan', { value: Number((e.currentTarget as HTMLInputElement).value) })}
              style="width:100%;height:40px;accent-color:#5fe3e0;cursor:pointer"
            />
          </label>
        {/if}
      </div>
    {/if}
    {#if vm.dcaSupported}
      <div style="display:flex;flex-direction:column;gap:4px">
        <span style="font-size:12px;color:#7f8a9c">DCA group</span>
        <div style="display:flex;gap:6px">
          {#each vm.dca as r}
            <button
              onclick={() => onEvent?.('dca', { group: r.group, member: !(r.member === true) })}
              aria-pressed={r.member === true ? 'true' : 'false'}
              aria-label={r.memberAria}
              title={r.member === null ? 'DCA membership not confirmed yet — tap to set' : undefined}
              style="box-sizing:border-box;font-variant-numeric:normal;flex:1;min-width:0;height:40px;border-radius:3px;border:1px solid #3a414c;background:{r.memberBg};color:{r.memberFg};font-size:12px;font-weight:600;cursor:pointer"
              >{r.member === null ? '?' : r.group + 1}</button
            >
          {/each}
        </div>
      </div>
    {/if}
    {#each [{ rows: vm.mg, kind: 'mg', heading: 'Mute group' }, { rows: vm.sg, kind: 'sg', heading: 'Subgroups' }] as sec}
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
    {#if vm.automixSupported}
      <div style="display:flex;flex-direction:column;gap:4px">
        <span style="font-size:12px;color:#7f8a9c">Automix</span>
        <button
          onclick={() => onEvent?.('automixOn', { on: !(vm.automixOn === true) })}
          aria-pressed={vm.automixOn === true ? 'true' : 'false'}
          aria-label={vm.automixOnAria}
          title={vm.automixOn === null ? 'Automix enable not confirmed yet — tap to set' : undefined}
          style="box-sizing:border-box;font-variant-numeric:normal;width:100%;height:40px;flex:none;border-radius:3px;border:1px solid #3a414c;background:{vm.automixOnBg};color:{vm.automixOnFg};font-size:12px;font-weight:600;cursor:pointer"
          >{vm.automixOnLabel}</button
        >
        <label style="display:flex;flex-direction:column;gap:4px">
          <span style="font-size:12px;color:#7f8a9c">Weight · {vm.automixWeightText}</span>
          <input
            type="range"
            min="0"
            max="100"
            step="1"
            value={vm.automixWeight === null ? 50 : Math.round(vm.automixWeight * 100)}
            aria-label={vm.automixWeightAria}
            title={vm.automixWeight === null ? 'Weight not confirmed yet — drag to set' : undefined}
            onchange={e => onEvent?.('automix', { value: Number((e.currentTarget as HTMLInputElement).value) / 100 })}
            style="width:100%;height:40px;accent-color:#5fe3e0;cursor:pointer"
          />
        </label>
        <div style="display:flex;align-items:baseline;justify-content:space-between">
          <span style="font-size:12px;color:#7f8a9c">Gain reduction</span>
          <span style="font-size:13px;font-weight:600;color:#eef2f7;font-variant-numeric:tabular-nums">{grText}</span>
        </div>
      </div>
    {/if}
    {#if vm.note}
      <div style="font-size:12px;color:#7f8a9c">{vm.note}</div>
    {/if}
  </div>
{/if}

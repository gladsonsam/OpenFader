<!-- `channel-presets-card`: built-in + user channel-strip presets.
     Presentational shell over ui/presets (core + localStorage): the card
     reads the target channel from the fixture store, recalls through the
     same ui.do setters as user gestures (so the live bridge forwards each
     leaf when online, and fixture mode just edits the model), and stores
     user presets in localStorage. No `on` wiring needed: behaviour is
     owned here, the layout only places the module. Unsupported leaves
     collapse into honest notes — never fake values. -->
<script lang="ts">
  import { ui } from '../../state/uiStore.svelte.ts';
  import { live } from '../../state/live.svelte.ts';
  import { bridgeStrip } from '../../state/engineBridge.ts';
  import type { ModuleProps } from '../../modules/types.ts';
  import {
    applyPreset,
    builtinPresets,
    captureChannel,
    diffSupport,
    summarizePreset,
    type ChannelPreset,
  } from '../../presets/channelPresets.ts';
  import {
    deleteUserPreset,
    loadUserPresets,
    persistUserPresets,
    renameUserPreset,
    upsertUserPreset,
  } from '../../presets/userPresets.ts';

  let { props }: ModuleProps<{ channelId?: string }> = $props();

  const builtins = builtinPresets();
  let userList = $state<ChannelPreset[]>(loadUserPresets());
  let name = $state('');
  let message = $state('');
  let renamingId = $state<string | null>(null);
  let renameText = $state('');

  const ch = $derived(
    props.channelId ? (ui.chans[props.channelId] ?? ui.sel) : ui.sel,
  );
  const locked = $derived(ui.state.locked);

  // Live support context (re-read on every engine ingest; null offline, so
  // fixture mode renders no desk badges at all).
  const supportCtx = $derived.by(() => {
    void live.version;
    const w = live.phase === 'online' ? live.engine : null;
    if (!w || !w.capsReady) return null;
    const mapped = bridgeStrip(w, ch.id);
    return { driver: w.driver, mapped, supports: (id: string) => w.supports(id, mapped ?? undefined) };
  });

  /** Short honesty note per preset (empty offline: nothing to disagree). */
  function supportNote(p: ChannelPreset): { text: string; title: string } {
    if (!supportCtx) return { text: '', title: '' };
    const rep = diffSupport(
      p,
      supportCtx.mapped !== null ? supportCtx.supports : null,
      supportCtx.mapped !== null,
    );
    const bits: string[] = [];
    if (supportCtx.mapped === null) bits.push('strip has no desk equivalent — recall stays local');
    if (rep.fixtureOnly.length) bits.push(rep.fixtureOnly.length + ' fixture-only');
    if (rep.deskRefused.length) bits.push(rep.deskRefused.length + ' local-only on ' + supportCtx.driver);
    if (!bits.length) return { text: '', title: '' };
    const leaves = [...rep.fixtureOnly, ...rep.deskRefused].join(', ');
    return { text: '△ ' + bits.join(' · '), title: leaves };
  }

  const missingSections = $derived(
    [!ch.input ? 'preamp' : null, !ch.eq ? 'EQ' : null, !ch.dyn ? 'dynamics' : null].filter(
      (s): s is string => s !== null,
    ),
  );

  function recall(p: ChannelPreset): void {
    const rep = applyPreset(ui.do, ch.id, ch, p, locked);
    message =
      rep.refused === 'locked'
        ? 'Locked — unlock to recall a preset.'
        : 'Recalled ' + p.label + ' · ' + rep.applied.length + ' leaves' +
          (rep.skipped.length ? ' · skipped: ' + rep.skipped.join('; ') : '');
  }

  function save(): void {
    const p = captureChannel(ch, name.trim() || ch.name);
    userList = upsertUserPreset(userList, p);
    persistUserPresets(userList);
    name = '';
    renamingId = null;
    message = 'Saved ' + p.label + ' · tone only (gain/EQ/dyn) — never 48V';
  }

  function remove(id: string): void {
    const p = userList.find(u => u.id === id);
    userList = deleteUserPreset(userList, id);
    persistUserPresets(userList);
    if (renamingId === id) renamingId = null;
    message = p ? 'Deleted ' + p.label + '.' : 'Deleted preset.';
  }

  function startRename(p: ChannelPreset): void {
    renamingId = p.id;
    renameText = p.label;
  }

  function commitRename(id: string): void {
    const next = renameUserPreset(userList, id, renameText);
    if (!next) {
      message = 'Rename refused — names cannot be blank.';
      return;
    }
    userList = next;
    persistUserPresets(userList);
    renamingId = null;
    message = 'Renamed preset.';
  }

  const btn =
    'box-sizing:border-box;min-height:44px;padding:8px 12px;border-radius:3px;font-size:14px;font-weight:600;cursor:pointer;border:1px solid #3a414c;';
  const presetBtn =
    btn + 'flex:1 1 150px;min-width:0;background:#1a2230;color:#e8edf5;text-align:left;display:flex;flex-direction:column;gap:2px;';
  const rowBtn = btn + 'background:#2b3038;color:#e8edf5;white-space:nowrap;';
</script>

<details
  aria-label="Channel presets"
  style="box-sizing:content-box;display:flex;flex-direction:column;gap:8px;padding:10px;border-radius:3px;border:1px solid #1f2836;background:#0e1520;min-width:0"
>
  <summary style="min-height:44px;align-content:center;cursor:pointer;font-size:15px;font-weight:600;color:#eef2f5">Channel presets</summary>
  <div style="display:flex;flex-direction:column;gap:8px">

  {#if missingSections.length}
    <div style="font-size:12px;color:#f0b44a">
      No {missingSections.join(' / ')} on this strip — those leaves skip on recall.
    </div>
  {/if}

  <div style="font-size:12px;font-weight:700;color:#8f9bb0">Built-in</div>
  <div style="display:flex;gap:6px;flex-wrap:wrap">
    {#each builtins as p (p.id)}
      {@const note = supportNote(p)}
      <button
        style={presetBtn}
        onclick={() => recall(p)}
        disabled={locked}
        title={locked ? 'Locked — unlock to recall' : note.title || summarizePreset(p)}
        aria-label={'Recall preset ' + p.label + (note.text ? ' (' + note.text + ')' : '')}
      >
        <span style="font-size:14px;font-weight:700;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">{p.label}</span>
        <span style="font-size:11px;font-weight:400;color:#8f9bb0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">
          {note.text || summarizePreset(p)}
        </span>
      </button>
    {/each}
  </div>

  <div style="font-size:12px;font-weight:700;color:#8f9bb0">Save current</div>
  <div style="display:flex;gap:6px;flex-wrap:wrap">
    <input
      bind:value={name}
      placeholder={ch.name + ' preset'}
      aria-label="New preset name"
      maxlength="32"
      style="box-sizing:border-box;flex:1 1 140px;min-width:0;min-height:44px;padding:8px 10px;border-radius:3px;border:1px solid #3a414c;background:#0e1520;color:#e8edf5;font-size:14px"
    />
    <button style={rowBtn + 'background:#5cdb5c;color:#10240f;border-color:#5cdb5c'} onclick={save} aria-label="Save current channel as preset">Save</button>
  </div>
  <div style="font-size:11px;color:#7f8a9c">Saves tone only — never fader/mute/pan/sends, never 48V.</div>

  {#if userList.length}
    <div style="font-size:12px;font-weight:700;color:#8f9bb0">Saved</div>
    <div style="display:flex;flex-direction:column;gap:6px">
      {#each userList as p (p.id)}
        {@const note = supportNote(p)}
        <div
          style="box-sizing:border-box;display:flex;align-items:center;gap:6px;flex-wrap:wrap;padding:8px;border-radius:3px;border:1px solid #222a36;background:#0b111b"
        >
          {#if renamingId === p.id}
            <input
              bind:value={renameText}
              aria-label="Rename preset"
              maxlength="32"
              style="box-sizing:border-box;flex:1 1 120px;min-width:0;min-height:44px;padding:8px 10px;border-radius:3px;border:1px solid #3a414c;background:#0e1520;color:#e8edf5;font-size:14px"
            />
            <button style={rowBtn} onclick={() => commitRename(p.id)} aria-label="Confirm rename">✓</button>
            <button style={rowBtn} onclick={() => (renamingId = null)} aria-label="Cancel rename">✕</button>
          {:else}
            <div style="flex:1 1 120px;min-width:0">
              <div style="font-size:14px;font-weight:700;color:#eef2f5;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">{p.label}</div>
              <div style="font-size:11px;color:#8f9bb0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap" title={note.title || summarizePreset(p)}>
                {note.text || summarizePreset(p)}
              </div>
            </div>
            <button style={rowBtn} onclick={() => recall(p)} disabled={locked} title={locked ? 'Locked — unlock to recall' : 'Recall ' + p.label} aria-label={'Recall preset ' + p.label}>Recall</button>
            <button style={rowBtn} onclick={() => startRename(p)} aria-label={'Rename preset ' + p.label}>Rename</button>
            <button style={rowBtn + 'color:#f0b44a;'} onclick={() => remove(p.id)} aria-label={'Delete preset ' + p.label}>Del</button>
          {/if}
        </div>
      {/each}
    </div>
  {/if}

  {#if message}
    <div style="font-size:12px;color:#c9d1dc" role="status">{message}</div>
  {/if}
  </div>
</details>

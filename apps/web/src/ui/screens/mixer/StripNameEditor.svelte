<!-- W5c module `strip-name-editor`: small in-strip popup above the name bar
     for renaming + recolouring a channel. Rendered when the layout `when`
     matches (`store.nameEdit == item.id`); the strip root stays selectable
     underneath, so the editor root stops click propagation (typing or
     picking a colour must not re-trigger select). Name applies per
     keystroke (`rename`); colours come from the shared console-tape palette
     (`recolor` with the tape text colour); Done / Escape closes (`close`). -->
<script lang="ts">
  import { CHANNEL_ICON_CATEGORIES, CHAN_ICON_NAMES, chanIcons } from '../../icons.ts';
  import { CHAN_TAPE } from '../../fixtures/designData.ts';
  import type { ModuleContext } from '../../modules/types.ts';

  let {
    props = {},
    ctx: _ctx,
    nodeId: _nodeId,
    onEvent,
  }: {
    props: {
      name?: string;
      selectedColor?: string;
      selectedIcon?: string | null;
      inline?: boolean;
    };
    ctx: ModuleContext;
    nodeId: string;
    onEvent?: (name: string, payload?: unknown) => void;
  } = $props();

  const selectedIcon = $derived(props.selectedIcon ?? null);
  let editing = $state(false);
  let draftName = $state('');
  let pickerOpen = $state(false);
  let iconSearch = $state('');
  const iconGroups = $derived.by(() => {
    const words = iconSearch.trim().toLowerCase().split(/\s+/).filter(Boolean);
    return CHANNEL_ICON_CATEGORIES.map(category => ({
      category,
      names: CHAN_ICON_NAMES.filter(name => {
        const icon = chanIcons[name];
        const searchable = `${icon.label} ${icon.category} ${icon.keywords}`.toLowerCase();
        return icon.category === category && words.every(word => searchable.includes(word));
      }),
    })).filter(group => group.names.length > 0);
  });
  const selectedLabel = $derived(CHAN_ICON_NAMES.find(name => name === selectedIcon));
  $effect(() => {
    if (!props.inline) pickerOpen = true;
  });
  $effect(() => {
    if (!editing) draftName = props.name ?? '';
  });
</script>

<!-- svelte-ignore a11y_interactive_supports_focus, a11y_click_events_have_key_events -->
<div
  role={props.inline ? 'group' : 'dialog'}
  aria-label="Edit channel name, colour and icon"
  onclick={(e) => e.stopPropagation()}
  onpointerdown={(e) => e.stopPropagation()}
  style="{props.inline ? '' : 'position:absolute;left:0;right:0;bottom:calc(100% + 6px);z-index:10;box-shadow:0 8px 20px rgba(0,0,0,.6);'}display:flex;flex-direction:column;gap:6px;padding:10px;border-radius:var(--u4-r-3);border:1px solid var(--u4-knob-hubline);background:var(--u4-well-bg-2)"
>
  {#if props.inline}
    <label for="channel-name-{_nodeId}" style="font-size:12px;color:var(--u4-text-6)">Channel name</label>
  {/if}
  <!-- svelte-ignore a11y_autofocus -->
  <input
    id="channel-name-{_nodeId}"
    type="text"
    aria-label="Channel name"
    value={draftName}
    maxlength={12}
    autofocus={!props.inline}
    autocomplete="off"
    autocapitalize="words"
    spellcheck={false}
    onfocus={() => { editing = true; }}
    onblur={() => { editing = false; }}
    oninput={(e) => {
      draftName = e.currentTarget.value;
      onEvent?.('rename', { value: draftName });
    }}
    onkeydown={(e) => {
      if (e.key === 'Escape') onEvent?.('close');
    }}
    onclick={(e) => e.stopPropagation()}
    onpointerdown={(e) => e.stopPropagation()}
    style="width:100%;box-sizing:border-box;height:44px;padding:0 8px;border-radius:3px;border:1px solid var(--u4-knob-hubline);background:var(--u4-panel-bg);color:var(--u4-text-1);font-size:16px;font-weight:600"
  />
  <details bind:open={pickerOpen}>
    <summary style="min-height:44px;cursor:pointer;align-content:center;color:var(--u4-text-1);font-size:14px">Colour & icon{selectedLabel ? ` · ${chanIcons[selectedLabel].label}` : ''}</summary>
  <div style="display:grid;grid-template-columns:repeat(auto-fit,minmax(44px,1fr));gap:4px" role="group" aria-label="Channel colour">
    {#each CHAN_TAPE as tape (tape.color)}
      <button
        type="button"
        aria-label="Colour {tape.color}"
        aria-pressed={props.selectedColor === tape.color ? 'true' : 'false'}
        class="tpress"
        onclick={(e) => {
          e.stopPropagation();
          onEvent?.('recolor', { color: tape.color, text: tape.text });
        }}
        style="height:28px;padding:0;border-radius:3px;border:{props.selectedColor === tape.color
          ? '2px solid var(--u4-seg-on-bg)'
          : '1px solid rgba(0,0,0,.5)'};background:{tape.color};cursor:pointer"
      ></button>
    {/each}
  </div>
  <div class="icon-tools">
    <input
      type="search"
      aria-label="Search channel icons"
      placeholder="Search instruments, vocals, outputs…"
      bind:value={iconSearch}
      onkeydown={(e) => { if (e.key === 'Escape') iconSearch = ''; }}
    />
    <button
      type="button"
      aria-label="No icon"
      aria-pressed={selectedIcon == null}
      class="no-icon tpress"
      onclick={() => onEvent?.('reicon', { icon: null })}
    >None</button>
  </div>
  <div class="icon-library" role="group" aria-label="Channel icon">
    {#each iconGroups as group (group.category)}
      <div class="icon-category">{group.category}</div>
      <div class="icon-grid">
        {#each group.names as iconName (iconName)}
          {@const def = chanIcons[iconName]}
          <button
            type="button"
            aria-label="Icon {def.label}"
            aria-pressed={selectedIcon === iconName}
            title={def.label}
            class="icon-choice tpress"
            onclick={() => onEvent?.('reicon', { icon: iconName })}
          >
            <svg width="32" height="32" viewBox={def.viewBox} aria-hidden="true">
              {@html def.body}
            </svg>
            <span>{def.label}</span>
          </button>
        {/each}
      </div>
    {:else}
      <p class="no-results">No icons found. Try an instrument or source name.</p>
    {/each}
  </div>
  </details>
  {#if !props.inline}
  <button
    type="button"
    class="tpress"
    onclick={(e) => {
      e.stopPropagation();
      onEvent?.('close');
    }}
    style="height:32px;border-radius:3px;border:1px solid var(--u4-line-ctl);background:var(--u4-btn-bg);color:var(--u4-text-4);font-size:13px;font-weight:600;cursor:pointer"
    >Done</button
  >
  {/if}
</div>

<style>
  .icon-tools { display: flex; gap: 6px; margin: 10px 0 4px; }
  .icon-tools input {
    width: 0; flex: 1; min-width: 0; height: 38px; padding: 0 8px;
    border: 1px solid var(--u4-line-ctl); border-radius: var(--u4-r-3);
    background: var(--u4-panel-bg); color: var(--u4-text-1); font: inherit; font-size: 12px;
  }
  .icon-tools input::placeholder { color: var(--u4-text-6); }
  .no-icon {
    padding: 0 10px; border: 1px solid var(--u4-line-ctl); border-radius: var(--u4-r-3);
    background: var(--u4-panel-bg); color: var(--u4-text-4); font: inherit; font-size: 12px;
  }
  .icon-library { max-height: 310px; overflow-y: auto; overscroll-behavior: contain; padding: 0 4px 4px 0; }
  .icon-category { padding: 10px 0 6px; color: var(--u4-text-4); font-size: 12px; font-weight: 600; }
  .icon-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(64px, 1fr)); gap: 5px; }
  .icon-choice {
    min-height: 67px; padding: 6px 2px 4px; display: flex; flex-direction: column;
    align-items: center; justify-content: center; gap: 4px; border: 1px solid var(--u4-line-ctl);
    border-radius: var(--u4-r-3); background: var(--u4-panel-bg); color: var(--u4-text-4); cursor: pointer;
  }
  .icon-choice span { font-size: 10px; line-height: 12px; text-align: center; }
  .icon-choice svg { flex-shrink: 0; }
  .icon-choice:hover, .no-icon:hover { background: var(--u4-btn-bg); }
  .icon-choice[aria-pressed="true"], .no-icon[aria-pressed="true"] {
    border-color: var(--u4-text-1); box-shadow: inset 0 0 0 1px var(--u4-text-1); color: var(--u4-text-1);
  }
  .icon-choice:focus-visible, .no-icon:focus-visible, .icon-tools input:focus-visible {
    outline: 2px solid var(--u4-text-1); outline-offset: 2px;
  }
  .no-results { color: var(--u4-text-4); padding: 12px 2px; font-size: 12px; line-height: 1.5; }
</style>

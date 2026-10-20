<!-- W5c module `strip-name-editor`: small in-strip popup above the name bar
     for renaming + recolouring a channel. Rendered when the layout `when`
     matches (`store.nameEdit == item.id`); the strip root stays selectable
     underneath, so the editor root stops click propagation (typing or
     picking a colour must not re-trigger select). Name applies per
     keystroke (`rename`); colours come from the shared console-tape palette
     (`recolor` with the tape text colour); Done / Escape closes (`close`). -->
<script lang="ts">
  import { CHAN_ICON_NAMES, chanIcons, type ChanIconName } from '../../icons.ts';
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
    <summary style="min-height:44px;cursor:pointer;align-content:center;color:var(--u4-text-1);font-size:14px">Colour & icon{selectedIcon ? ` · ${selectedIcon}` : ''}</summary>
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
  <div style="display:grid;grid-template-columns:repeat(auto-fit,minmax(44px,1fr));gap:4px;margin-top:6px" role="group" aria-label="Channel icon">
    <button
      type="button"
      aria-label="No icon"
      aria-pressed={selectedIcon == null ? 'true' : 'false'}
      class="tpress"
      onclick={(e) => {
        e.stopPropagation();
        onEvent?.('reicon', { icon: null });
      }}
      style="height:32px;padding:0;border-radius:3px;border:{selectedIcon == null
        ? '2px solid var(--u4-seg-on-bg)'
        : '1px solid var(--u4-line-ctl)'};background:transparent;color:var(--u4-text-dim);font-size:11px;font-weight:600;cursor:pointer"
      >None</button
    >
    {#each CHAN_ICON_NAMES as iconName (iconName)}
      {@const def = chanIcons[iconName as ChanIconName]}
      <button
        type="button"
        aria-label="Icon {iconName}"
        aria-pressed={selectedIcon === iconName ? 'true' : 'false'}
        class="tpress"
        onclick={(e) => {
          e.stopPropagation();
          onEvent?.('reicon', { icon: iconName });
        }}
        style="height:32px;padding:0;border-radius:3px;display:flex;align-items:center;justify-content:center;border:{selectedIcon ===
        iconName
          ? '2px solid var(--u4-seg-on-bg)'
          : '1px solid var(--u4-line-ctl)'};background:transparent;color:var(--u4-text-1);cursor:pointer"
      >
        <svg
          width="20"
          height="20"
          viewBox={def.viewBox}
          fill="none"
          stroke="currentColor"
          stroke-width={def.strokeWidth}
          stroke-linecap={def.linecap ?? 'butt'}
          stroke-linejoin={def.linejoin ?? 'miter'}
          aria-hidden="true"
        >
          {@html def.body}
        </svg>
      </button>
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

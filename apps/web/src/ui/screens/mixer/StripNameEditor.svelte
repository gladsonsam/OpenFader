<!-- W5c module `strip-name-editor`: small in-strip popup above the name bar
     for renaming + recolouring a channel. Rendered when the layout `when`
     matches (`store.nameEdit == item.id`); the strip root stays selectable
     underneath, so the editor root stops click propagation (typing or
     picking a colour must not re-trigger select). Name applies per
     keystroke (`rename`); colours come from the shared console-tape palette
     (`recolor` with the tape text colour); Done / Escape closes (`close`). -->
<script lang="ts">
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
    };
    ctx: ModuleContext;
    nodeId: string;
    onEvent?: (name: string, payload?: unknown) => void;
  } = $props();
</script>

<div
  role="dialog"
  aria-label="Edit channel name and colour"
  onclick={(e) => e.stopPropagation()}
  onpointerdown={(e) => e.stopPropagation()}
  style="position:absolute;left:0;right:0;bottom:calc(100% + 6px);z-index:10;display:flex;flex-direction:column;gap:6px;padding:8px;border-radius:var(--u4-r-3);border:1px solid #2f3b4f;background:#0b111b;box-shadow:0 8px 20px rgba(0,0,0,.6)"
>
  <input
    type="text"
    aria-label="Channel name"
    defaultValue={props.name ?? ''}
    maxlength={12}
    autofocus
    autocomplete="off"
    autocapitalize="words"
    spellcheck={false}
    oninput={(e) => onEvent?.('rename', { value: e.currentTarget.value })}
    onkeydown={(e) => {
      if (e.key === 'Escape') onEvent?.('close');
    }}
    onclick={(e) => e.stopPropagation()}
    onpointerdown={(e) => e.stopPropagation()}
    style="width:100%;box-sizing:border-box;height:36px;padding:0 8px;border-radius:3px;border:1px solid #2f3b4f;background:#0e1520;color:var(--u4-text-1);font-size:15px;font-weight:600"
  />
  <div style="display:grid;grid-template-columns:repeat(8, 1fr);gap:4px" role="group" aria-label="Channel colour">
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
          ? '2px solid #ffffff'
          : '1px solid rgba(0,0,0,.5)'};background:{tape.color};cursor:pointer"
      ></button>
    {/each}
  </div>
  <button
    type="button"
    class="tpress"
    onclick={(e) => {
      e.stopPropagation();
      onEvent?.('close');
    }}
    style="height:32px;border-radius:3px;border:1px solid #3a414c;background:#2b3038;color:#c9d1dc;font-size:13px;font-weight:600;cursor:pointer"
    >Done</button
  >
</div>

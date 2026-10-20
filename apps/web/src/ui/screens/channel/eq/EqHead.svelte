<!-- W5b `eq-head`: the EQ tab head row (Presets button opening the section
     menu). Presentational: toggle visuals in via layout `bind` (label feeds
     the dialog Toggle row), `toggle` / `flat` / `reset` events out (all from
     the dialog rows). Button chrome verbatim from design-template.html
     (40px, radius 5, 15px) — the generic `btn` leaf hard-codes radius 3,
     so these buttons stay a dedicated channel module. The menu dialog
     copies BusPopup (mixer): fixed inset backdrop, Esc/backdrop close,
     420px panel, 52px rows. Locked disables the opener and the rows
     (store also blocks the edits). -->
<script lang="ts">
  import type { ModuleContext } from '../../../modules/types.ts';

  let {
    props = {},
    ctx: _ctx,
    nodeId: _nodeId,
    onEvent,
  }: {
    props: {
      label?: string;
      bg?: string;
      fg?: string;
      pressed?: string;
      ariaLabel?: string;
      locked?: boolean;
    };
    ctx: ModuleContext;
    nodeId: string;
    onEvent?: (name: string, payload?: unknown) => void;
  } = $props();

  const locked = $derived(props.locked ?? false);

  let open = $state(false);

  function pick(name: string): void {
    if (locked) return;
    open = false;
    onEvent?.(name);
  }
</script>

<div style="box-sizing:content-box;display:flex;align-items:center;gap:8px">
  <button
    onclick={() => { if (!locked) open = true; }}
    disabled={locked}
    aria-label="EQ presets"
    aria-haspopup="menu"
    aria-expanded={open ? 'true' : 'false'}
    style="box-sizing:border-box;display:flex;align-items:center;gap:6px;font-variant-numeric:normal;height:40px;padding:0 16px;border-radius:5px;border:1px solid var(--u4-line-ctl);background:{props.bg ??
      'var(--u4-btn-bg)'};color:{props.fg ?? 'var(--u4-text-1)'};font-size:15px;cursor:{locked
      ? 'not-allowed'
      : 'pointer'};opacity:{locked ? 0.45 : 1}"
    ><span>Presets</span><svg
      width="16"
      height="16"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="2.2"
      stroke-linecap="round"
      stroke-linejoin="round"
      aria-hidden="true"
      style="display:block"><path d="M6 9l6 6 6-6"></path></svg
    ></button
  >
</div>

{#if open}
  <!-- svelte-ignore a11y_interactive_supports_focus -->
  <div
    role="dialog"
    aria-label="EQ options"
    style="position:fixed;inset:0;z-index:60;display:flex;align-items:center;justify-content:center;background:rgba(4,8,14,0.72);padding:16px"
    onclick={e => {
      if (e.target === e.currentTarget) open = false;
    }}
    onkeydown={e => {
      if (e.key === 'Escape') open = false;
    }}
  >
    <div
      role="menu"
      style="box-sizing:content-box;width:min(420px,100%);max-height:min(560px,90vh);overflow-y:auto;display:flex;flex-direction:column;gap:6px;padding:14px;border-radius:6px;border:1px solid #2a3443;background:#0b111b"
    >
      <div style="display:flex;align-items:center;gap:8px">
        <span style="font-size:16px;font-weight:700;color:#eef2f5;margin-right:auto">EQ</span>
        <button
          onclick={() => (open = false)}
          aria-label="Close EQ options"
          style="box-sizing:border-box;width:36px;height:36px;border-radius:3px;border:1px solid #3a414c;background:#2b3038;color:#e8edf5;font-size:16px;cursor:pointer"
          >✕</button
        >
      </div>
      <button
        role="menuitem"
        onclick={() => pick('toggle')}
        disabled={locked}
        style="box-sizing:border-box;display:flex;align-items:center;gap:10px;min-height:52px;padding:8px 10px;border-radius:3px;border:1px solid #222a36;background:#0e1520;cursor:{locked
          ? 'not-allowed'
          : 'pointer'};text-align:left;opacity:{locked ? 0.45 : 1}"
      >
        <span style="flex:1 1 auto;min-width:0">
          <span style="display:block;font-size:14px;font-weight:700;color:#eef2f5">{props.label ?? ''}</span>
          <span style="display:block;font-size:11px;color:#8792a4">Toggle bypass</span>
        </span>
      </button>
      <button
        role="menuitem"
        onclick={() => pick('flat')}
        disabled={locked}
        style="box-sizing:border-box;display:flex;align-items:center;gap:10px;min-height:52px;padding:8px 10px;border-radius:3px;border:1px solid #222a36;background:#0e1520;cursor:{locked
          ? 'not-allowed'
          : 'pointer'};text-align:left;opacity:{locked ? 0.45 : 1}"
      >
        <span style="flex:1 1 auto;min-width:0">
          <span style="display:block;font-size:14px;font-weight:700;color:#eef2f5">Flatten gains</span>
          <span style="display:block;font-size:11px;color:#8792a4">All band gains to 0 dB</span>
        </span>
      </button>
      <button
        role="menuitem"
        onclick={() => pick('reset')}
        disabled={locked}
        style="box-sizing:border-box;display:flex;align-items:center;gap:10px;min-height:52px;padding:8px 10px;border-radius:3px;border:1px solid #222a36;background:#0e1520;cursor:{locked
          ? 'not-allowed'
          : 'pointer'};text-align:left;opacity:{locked ? 0.45 : 1}"
      >
        <span style="flex:1 1 auto;min-width:0">
          <span style="display:block;font-size:14px;font-weight:700;color:#eef2f5">Reset full EQ</span>
          <span style="display:block;font-size:11px;color:#8792a4">F/Q defaults · HPF 80 Hz · switched in</span>
        </span>
      </button>
    </div>
  </div>
{/if}

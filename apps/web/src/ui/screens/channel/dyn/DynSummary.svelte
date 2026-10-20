<!-- W5b `dyn-summary` module: the dyn tab top row (Presets button + summary).
     Presentational: summary in via layout `bind`, `toggle` / `reset`
     events out (from the section menu dialog). Verbatim row chrome from
     design-template.html.
     The menu dialog copies BusPopup (mixer): fixed inset backdrop,
     Esc/backdrop close, 420px panel, 52px rows. Locked disables the
     opener and the rows (store also blocks the edits).
     TODO: per-section presets. Channel presets (channelPresets.ts) are
     full-strip only; a Gate/Comp preset row needs a section-preset store
     first. Until then the menu is Toggle + Reset. -->
<script lang="ts">
  import type { ModuleContext } from '../../../modules/types.ts';

  let {
    props = {},
    ctx: _ctx,
    nodeId: _nodeId,
    onEvent,
  }: {
    props: {
      kind?: string;
      label?: string;
      bg?: string;
      fg?: string;
      pressed?: string;
      summary?: string;
      locked?: boolean;
      /** Phone stacked layout: the summary drops full-width below the
       *  button (left-aligned, no min-width) instead of overlapping it. */
      compact?: boolean;
    };
    ctx: ModuleContext;
    nodeId: string;
    onEvent?: (name: string, payload?: unknown) => void;
  } = $props();

  const compact = $derived(props.compact ?? false);
  const locked = $derived(props.locked ?? false);
  const kindLabel = $derived(props.kind === 'gate' ? 'Gate' : 'Compressor');

  let open = $state(false);

  function pick(name: string): void {
    if (locked) return;
    open = false;
    onEvent?.(name);
  }

  // Wrap only between "·" segments: each "value unit" chunk stays on one
  // line so a unit (dB, ms, Hz) can never strand onto its own line.
  const parts = $derived((props.summary ?? '').split(' · '));
</script>

<div
  style="box-sizing:content-box;flex:none;display:flex;flex-wrap:wrap;align-items:center;gap:8px"
>
  <span style="display:flex;align-items:stretch">
    <button
      onclick={() => { if (!locked) open = true; }}
      disabled={locked}
      aria-label={kindLabel + ' presets'}
      aria-haspopup="menu"
      aria-expanded={open ? 'true' : 'false'}
      style="box-sizing:border-box;display:flex;align-items:center;gap:6px;font-variant-numeric:normal;height:40px;padding:0 16px;border-radius:3px;border:1px solid var(--u4-line-ctl);background:{props.bg ??
        'var(--u4-btn-bg)'};color:{props.fg ?? 'var(--u4-text-4)'};font-size:15px;cursor:{locked
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
  </span>
  <div
    style="box-sizing:content-box;flex:1;min-width:{compact ? 0 : 160}px{compact ? ';flex-basis:100%' : ''};text-align:{compact ? 'left' : 'right'};font-size:13px;color:var(--u4-text-4)"
  >
    {#each parts as p, i}
      {#if i > 0}<span aria-hidden="true">{' · '}</span>{/if}<span style="white-space:nowrap">{p}</span>
    {/each}
  </div>
</div>

{#if open}
  <!-- svelte-ignore a11y_interactive_supports_focus -->
  <div
    role="dialog"
    aria-label={kindLabel + ' options'}
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
      style="box-sizing:content-box;width:min(420px,100%);max-height:min(560px,90vh);overflow-y:auto;display:flex;flex-direction:column;gap:6px;padding:14px;border-radius:6px;border:1px solid var(--u4-detail-line, #2a3443);background:var(--u4-detail-well, #0b111b)"
    >
      <div style="display:flex;align-items:center;gap:8px">
        <span style="font-size:16px;font-weight:700;color:var(--u4-detail-heading, #eef2f5);margin-right:auto">{kindLabel}</span>
        <button
          onclick={() => (open = false)}
          aria-label={'Close ' + kindLabel + ' options'}
          style="box-sizing:border-box;width:36px;height:36px;border-radius:3px;border:1px solid var(--u4-detail-line, #3a414c);background:var(--u4-detail-button, #2b3038);color:var(--u4-detail-heading, #e8edf5);font-size:16px;cursor:pointer"
          >✕</button
        >
      </div>
      <button
        role="menuitem"
        onclick={() => pick('toggle')}
        disabled={locked}
        style="box-sizing:border-box;display:flex;align-items:center;gap:10px;min-height:52px;padding:8px 10px;border-radius:3px;border:1px solid var(--u4-detail-line, #222a36);background:var(--u4-detail-panel, #0e1520);cursor:{locked
          ? 'not-allowed'
          : 'pointer'};text-align:left;opacity:{locked ? 0.45 : 1}"
      >
        <span style="flex:1 1 auto;min-width:0">
          <span style="display:block;font-size:14px;font-weight:700;color:var(--u4-detail-heading, #eef2f5)">{props.label ?? ''}</span>
          <span style="display:block;font-size:11px;color:var(--u4-detail-muted, #8792a4)">Toggle bypass</span>
        </span>
      </button>
      <button
        role="menuitem"
        onclick={() => pick('reset')}
        disabled={locked}
        style="box-sizing:border-box;display:flex;align-items:center;gap:10px;min-height:52px;padding:8px 10px;border-radius:3px;border:1px solid var(--u4-detail-line, #222a36);background:var(--u4-detail-panel, #0e1520);cursor:{locked
          ? 'not-allowed'
          : 'pointer'};text-align:left;opacity:{locked ? 0.45 : 1}"
      >
        <span style="flex:1 1 auto;min-width:0">
          <span style="display:block;font-size:14px;font-weight:700;color:var(--u4-detail-heading, #eef2f5)">Reset</span>
          <span style="display:block;font-size:11px;color:var(--u4-detail-muted, #8792a4)">Factory defaults, switched in</span>
        </span>
      </button>
    </div>
  </div>
{/if}

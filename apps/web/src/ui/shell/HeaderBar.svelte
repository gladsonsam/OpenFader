<!-- H1 HeaderBar: the 56px top header, copied verbatim from
     docs/design/src/design-template.html (header block, lines 21-72).
     Props: vm = the store's headerView selector (ui.header), mode for the
     toolbar/scene active states, and callbacks (no store import: presentational).
     Layout: [Mixer title | Back + channel chip / page title | lock banner]
     spacer [FX Routing Connect Settings] divider [scene button]. -->
<!-- NOTE: buttons carry font-variant-numeric:normal because the design's
     buttons compute normal (UA `font:` shorthand reset; the template only
     re-inherits font-family), while app.css forces tabular on buttons. The
     header itself is content-box so its 56px height + 1px border = 57px,
     exactly like the template (measured on the live design). -->
<script lang="ts">
  import type { HeaderView, Mode } from '../state/uiLogic.ts';
  import Icon from '../primitives/Icon.svelte';
  import SceneButton from './SceneButton.svelte';
  import { ariaCurrent, tileStyle } from './shellStyle.ts';

  let {
    vm,
    mode,
    onBack,
    onUnlock,
    onFx,
    onRouting,
    onConnect,
    onSettings,
    onScene,
    onMeters = null,
    metersActive = false,
    compact = false,
  }: {
    vm: HeaderView;
    mode: Mode;
    onBack: () => void;
    onUnlock: () => void;
    onFx: () => void;
    onRouting: () => void;
    onConnect: () => void;
    onSettings: () => void;
    onScene: () => void;
    /** Meter-bridge toggle (null = hide the button). */
    onMeters?: (() => void) | null;
    /** Meter-bridge overlay open (toolbar active state). */
    metersActive?: boolean;
    /** Phone density: hide redundant titles, narrow the scene button. */
    compact?: boolean;
  } = $props();

  const fx = $derived(tileStyle(mode === 'fx'));
  const rt = $derived(tileStyle(mode === 'routing'));
  const cn = $derived(tileStyle(mode === 'connect'));
  const st = $derived(tileStyle(mode === 'settings'));
  const mb = $derived(tileStyle(metersActive));
</script>

<header
  style={compact
    ? 'min-height:var(--u4-header-h);height:auto;box-sizing:content-box;flex:none;display:flex;align-items:center;flex-wrap:wrap;gap:4px 8px;padding:4px 8px 4px 12px;background:var(--u4-header-bg);border-bottom:1px solid var(--u4-header-border)'
    : 'height:var(--u4-header-h);box-sizing:content-box;flex:none;display:flex;align-items:center;gap:8px;padding:0 8px 0 12px;background:var(--u4-header-bg);border-bottom:1px solid var(--u4-header-border)'}
>
  {#if !vm.showBack && !compact}
    <div style="font-size:17px;font-weight:600;letter-spacing:.02em">Mixer</div>
  {/if}
  {#if vm.showBack}
    <button
      onclick={onBack}
      aria-label="Back"
      style="height:46px;padding:0 14px 0 10px;display:flex;align-items:center;gap:4px;border-radius:var(--u4-r-3);border:1px solid var(--u4-line-ctl-2);background:var(--u4-bank-off);color:var(--u4-text-1);font-size:14px;cursor:pointer;font-variant-numeric:normal"
    >
      <Icon name="back" size={18} />
      {vm.backLabel}
    </button>
  {/if}
  {#if vm.isChannel}
    <div style="display:flex;align-items:center;gap:8px;white-space:nowrap;min-width:0">
      <span style="width:10px;height:10px;flex:none;border-radius:var(--u4-r-2);background:{vm.channelColor}"></span>
      <span style="font-size:17px;font-weight:600">{vm.channelName}</span>
      {#if !compact}<span style="font-size:14px;color:var(--u4-text-5)">{vm.channelTabLabel}</span>{/if}
    </div>
  {/if}
  {#if vm.showPageTitle && !compact}
    <div style="font-size:17px;font-weight:600;white-space:nowrap">{vm.pageTitle}</div>
  {/if}
  {#if vm.locked}
    <button
      onclick={onUnlock}
      style="height:40px;display:flex;align-items:center;gap:6px;padding:0 12px;border-radius:var(--u4-r-3);border:1px solid var(--u4-amber);background:var(--u4-amber-deep);color:var(--u4-amber-hi);font-size:13px;font-weight:600;cursor:pointer;white-space:nowrap;font-variant-numeric:normal"
    >
      <Icon name="lock" size={16} />
      Locked · tap to unlock
    </button>
  {/if}

  <div style="flex:1 1 0;min-width:4px"></div>

  <div role="toolbar" aria-label="Console tools" style="display:flex;gap:4px;flex:none">
    <button
      onclick={onFx}
      aria-label="Effects"
      aria-current={ariaCurrent(mode === 'fx')}
      style="width:var(--u4-iconbtn);height:var(--u4-iconbtn);border-radius:var(--u4-r-3);border:1px solid {fx.border};background:{fx.bg};color:var(--u4-text-3);cursor:pointer;font-size:16px;font-weight:600;padding:0;font-variant-numeric:normal"
      >FX</button
    >
    <button
      onclick={onRouting}
      aria-label="Routing"
      aria-current={ariaCurrent(mode === 'routing')}
      style="width:var(--u4-iconbtn);height:var(--u4-iconbtn);border-radius:var(--u4-r-3);border:1px solid {rt.border};background:{rt.bg};color:var(--u4-text-3);cursor:pointer;display:flex;align-items:center;justify-content:center;padding:0;font-variant-numeric:normal"
    >
      <Icon name="routing" size={22} />
    </button>
    <button
      onclick={onConnect}
      aria-label={vm.connAria}
      aria-current={ariaCurrent(mode === 'connect')}
      style="width:var(--u4-iconbtn);height:var(--u4-iconbtn);position:relative;border-radius:var(--u4-r-3);border:1px solid {cn.border};background:{cn.bg};color:var(--u4-text-3);cursor:pointer;display:flex;align-items:center;justify-content:center;padding:0;font-variant-numeric:normal"
    >
      <Icon name="plug" size={22} />
      <span
        style="position:absolute;right:6px;top:6px;width:8px;height:8px;border-radius:50%;background:{vm.connDot};box-shadow:0 0 0 2px var(--u4-tile-bg)"
      ></span>
    </button>
    <button
      onclick={onSettings}
      aria-label="Settings"
      aria-current={ariaCurrent(mode === 'settings')}
      style="width:var(--u4-iconbtn);height:var(--u4-iconbtn);border-radius:var(--u4-r-3);border:1px solid {st.border};background:{st.bg};color:var(--u4-text-3);cursor:pointer;display:flex;align-items:center;justify-content:center;padding:0;font-variant-numeric:normal"
    >
      <Icon name="gear" size={22} />
    </button>
    {#if onMeters}
      <button
        onclick={onMeters}
        aria-label="Meter bridge"
        aria-current={ariaCurrent(metersActive)}
        style="width:var(--u4-iconbtn);height:var(--u4-iconbtn);border-radius:var(--u4-r-3);border:1px solid {mb.border};background:{mb.bg};color:var(--u4-text-3);cursor:pointer;display:flex;align-items:center;justify-content:center;padding:0;font-variant-numeric:normal"
      >
        <Icon name="meters" size={22} />
      </button>
    {/if}
  </div>

  <div aria-hidden="true" style="width:1px;height:30px;flex:none;background:var(--u4-divider)"></div>

  <SceneButton num={vm.curSceneNum} name={vm.curSceneName} label={vm.sceneAria} active={vm.isScenes} narrow={compact} onOpen={onScene} />
</header>

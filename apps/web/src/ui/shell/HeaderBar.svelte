<!-- H1 HeaderBar: the 56px top header, copied verbatim from
     docs/design/src/design-template.html (header block, lines 21-72).
     Props: vm = the store's headerView selector (ui.header), mode for the
     toolbar/scene active states, and callbacks (no store import: presentational).
     Layout: [Mixer title | Back + channel chip / page title | lock banner]
     spacer [FX Routing Connect Settings] divider [scene button].
     Channel mode strips to [Back | centered channel name | prev/next]: the toolbar,
     scene button and lock banner stay one Back tap away on the mixer.
     Snapshot visibility: phone (compact) renders the scene button ONLY on
     the mixer with a desk link up (connected); every other phone screen
     hides it so the 36px bar stays one row (nowrap + overflow hidden).
     Desktop keeps the design's button on all non-channel screens. -->
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
    connected = true,
    onPrevChannel = null,
    onNextChannel = null,
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
    /** Desk link up (live relay online or a fixture desk selected — not
        explicit offline). The snapshot button only renders when true. */
    connected?: boolean;
    /** Channel prev/next (rendered only in channel mode when both set). */
    onPrevChannel?: (() => void) | null;
    onNextChannel?: (() => void) | null;
  } = $props();

  const fx = $derived(tileStyle(mode === 'fx'));
  const rt = $derived(tileStyle(mode === 'routing'));
  const cn = $derived(tileStyle(mode === 'connect'));
  const st = $derived(tileStyle(mode === 'settings'));
  const mb = $derived(tileStyle(metersActive));
  // Snapshot (scene) button visibility: phone shows it ONLY on the mixer
  // with a desk link up (everywhere else it wrapped the 36px bar to two
  // rows). Desktop has room so it keeps the design's button on every
  // non-channel screen, but still hides it with no desk link.
  const showScene = $derived(
    compact ? mode === 'mixer' && connected : !vm.isChannel && connected,
  );
  // Phone density: 36px touch rows so Back + title + toolbar share one
  // row instead of stacking. Desktop untouched.
  const ibtn = $derived(compact ? 36 : 46);
</script>

<header
  style={compact
    ? 'min-height:36px;height:auto;box-sizing:content-box;flex:none;display:flex;align-items:center;flex-wrap:nowrap;overflow:hidden;gap:2px 6px;padding:2px 6px 2px 8px;background:var(--u4-header-bg);border-bottom:1px solid var(--u4-header-border)'
    : 'height:var(--u4-header-h);box-sizing:content-box;flex:none;display:flex;align-items:center;gap:8px;padding:0 8px 0 12px;background:var(--u4-header-bg);border-bottom:1px solid var(--u4-header-border)'}
>
  {#if !vm.showBack && !compact}
    <div style="font-size:17px;font-weight:600;letter-spacing:.02em">Mixer</div>
  {/if}
  {#if vm.showBack}
    <button
      onclick={onBack}
      aria-label="Back"
      style="box-sizing:border-box;height:{compact ? 36 : 46}px;padding:0 {compact ? '10px 0 8px' : '14px 0 10px'};display:flex;align-items:center;gap:4px;border-radius:var(--u4-r-3);border:1px solid var(--u4-line-ctl-2);background:var(--u4-bank-off);color:var(--u4-text-1);font-size:14px;cursor:pointer;font-variant-numeric:normal"
    >
      <Icon name="back" size={18} />
      {vm.backLabel}
    </button>
  {/if}
  {#if vm.isChannel}
    <div style="flex:1 1 0;display:flex;align-items:center;justify-content:center;gap:8px;white-space:nowrap;min-width:0;text-align:center">
      <span style="width:10px;height:10px;flex:none;border-radius:var(--u4-r-2);background:{vm.channelColor}"></span>
      <span style="font-size:17px;font-weight:600;overflow:hidden;text-overflow:ellipsis">{vm.channelName}</span>
    </div>
  {/if}
  {#if vm.showPageTitle && !compact}
    <div style="font-size:17px;font-weight:600;white-space:nowrap">{vm.pageTitle}</div>
  {/if}
  {#if vm.locked && !vm.isChannel}
    <button
      onclick={onUnlock}
      style="box-sizing:border-box;height:{compact ? 36 : 46}px;display:flex;align-items:center;gap:6px;padding:0 12px;border-radius:var(--u4-r-3);border:1px solid var(--u4-amber);background:var(--u4-amber-deep);color:var(--u4-amber-hi);font-size:13px;font-weight:600;cursor:pointer;white-space:nowrap;font-variant-numeric:normal"
    >
      <Icon name="lock" size={16} />
      Locked · tap to unlock
    </button>
  {/if}

  {#if !vm.isChannel}
    <div style="flex:1 1 0;min-width:4px"></div>
  {/if}

  {#if vm.isChannel && onPrevChannel && onNextChannel}
    <div role="group" aria-label="Channel" style="display:flex;gap:4px;flex:none">
      <button
        onclick={onPrevChannel}
        aria-label="Previous channel"
        style="box-sizing:border-box;height:{compact ? 36 : 46}px;width:{compact ? 36 : 46}px;display:flex;align-items:center;justify-content:center;border-radius:var(--u4-r-3);border:1px solid var(--u4-line-ctl-2);background:var(--u4-bank-off);color:var(--u4-text-1);cursor:pointer;font-variant-numeric:normal"
      >
        <Icon name="chevronLeft" size={18} />
      </button>
      <button
        onclick={onNextChannel}
        aria-label="Next channel"
        style="box-sizing:border-box;height:{compact ? 36 : 46}px;width:{compact ? 36 : 46}px;display:flex;align-items:center;justify-content:center;border-radius:var(--u4-r-3);border:1px solid var(--u4-line-ctl-2);background:var(--u4-bank-off);color:var(--u4-text-1);cursor:pointer;font-variant-numeric:normal"
      >
        <Icon name="chevronRight" size={18} />
      </button>
    </div>
  {/if}

  {#if !vm.isChannel}
  <div role="toolbar" aria-label="Console tools" style="display:flex;gap:{compact ? 2 : 4}px;flex:none">
    <button
      onclick={onFx}
      aria-label="Effects"
      aria-current={ariaCurrent(mode === 'fx')}
      style="box-sizing:border-box;width:{ibtn}px;height:{ibtn}px;border-radius:var(--u4-r-3);border:1px solid {fx.border};background:{fx.bg};color:var(--u4-text-3);cursor:pointer;font-size:16px;font-weight:600;padding:0;font-variant-numeric:normal"
      >FX</button
    >
    <button
      onclick={onRouting}
      aria-label="Routing"
      aria-current={ariaCurrent(mode === 'routing')}
      style="box-sizing:border-box;width:{ibtn}px;height:{ibtn}px;border-radius:var(--u4-r-3);border:1px solid {rt.border};background:{rt.bg};color:var(--u4-text-3);cursor:pointer;display:flex;align-items:center;justify-content:center;padding:0;font-variant-numeric:normal"
    >
      <Icon name="routing" size={compact ? 18 : 22} />
    </button>
    <button
      onclick={onConnect}
      aria-label={vm.connAria}
      aria-current={ariaCurrent(mode === 'connect')}
      style="box-sizing:border-box;width:{ibtn}px;height:{ibtn}px;position:relative;border-radius:var(--u4-r-3);border:1px solid {cn.border};background:{cn.bg};color:var(--u4-text-3);cursor:pointer;display:flex;align-items:center;justify-content:center;padding:0;font-variant-numeric:normal"
    >
      <Icon name="plug" size={compact ? 18 : 22} />
      <span
        style="position:absolute;right:6px;top:6px;width:8px;height:8px;border-radius:50%;background:{vm.connDot};box-shadow:0 0 0 2px var(--u4-tile-bg)"
      ></span>
    </button>
    <button
      onclick={onSettings}
      aria-label="Settings"
      aria-current={ariaCurrent(mode === 'settings')}
      style="box-sizing:border-box;width:{ibtn}px;height:{ibtn}px;border-radius:var(--u4-r-3);border:1px solid {st.border};background:{st.bg};color:var(--u4-text-3);cursor:pointer;display:flex;align-items:center;justify-content:center;padding:0;font-variant-numeric:normal"
    >
      <Icon name="gear" size={compact ? 18 : 22} />
    </button>
    {#if onMeters}
      <button
        onclick={onMeters}
        aria-label="Meter bridge"
        aria-current={ariaCurrent(metersActive)}
        style="box-sizing:border-box;width:{ibtn}px;height:{ibtn}px;border-radius:var(--u4-r-3);border:1px solid {mb.border};background:{mb.bg};color:var(--u4-text-3);cursor:pointer;display:flex;align-items:center;justify-content:center;padding:0;font-variant-numeric:normal"
      >
        <Icon name="meters" size={compact ? 18 : 22} />
      </button>
    {/if}
  </div>
  {/if}

  {#if !compact && showScene}
    <div aria-hidden="true" style="width:1px;height:30px;flex:none;background:var(--u4-divider)"></div>
  {/if}

  {#if showScene}
    <SceneButton num={vm.curSceneNum} name={vm.curSceneName} label={vm.sceneAria} active={vm.isScenes} narrow={compact} onOpen={onScene} />
  {/if}
</header>

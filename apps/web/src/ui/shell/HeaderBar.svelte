<!-- Header controls share a 44px target with 6px of vertical breathing room.
     Channel pages replace the console toolbar with channel navigation. -->
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
    preview = false,
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
    preview?: boolean;
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
  const ibtn = 44;
</script>

<header
  class="u4-header"
  style:padding-top="calc(var(--app-safe-top, 0px) + 6px)"
  style:padding-left="calc(var(--app-safe-left, 0px) + {compact ? 8 : 12}px)"
  style:padding-right="calc(var(--app-safe-right, 0px) + {compact ? 6 : 8}px)"
  style="--u4-header-button-size:{ibtn}px;min-height:var(--u4-header-h);height:auto;box-sizing:border-box;flex:none;display:flex;align-items:center;flex-wrap:nowrap;gap:{compact ? 6 : 8}px;padding:6px {compact ? 6 : 8}px 6px {compact ? 8 : 12}px;background:var(--u4-header-bg);border-bottom:1px solid var(--u4-header-border)"
>
  {#if vm.showBack}
    <button
      onclick={onBack}
      aria-label="Back"
      style="box-sizing:border-box;height:{ibtn}px;flex:none;padding:0 {compact ? '10px 0 8px' : '14px 0 10px'};display:flex;align-items:center;gap:4px;border-radius:var(--u4-r-3);border:1px solid var(--u4-line-ctl-2);background:var(--u4-bank-off);color:var(--u4-text-1);font-size:14px;cursor:pointer;font-variant-numeric:normal"
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
  {#if vm.locked && !vm.isChannel}
    <button
      onclick={onUnlock}
      style="box-sizing:border-box;height:{ibtn}px;display:flex;align-items:center;gap:6px;padding:0 12px;border-radius:var(--u4-r-3);border:1px solid var(--u4-amber);background:var(--u4-amber-deep);color:var(--u4-amber-hi);font-size:13px;font-weight:600;cursor:pointer;white-space:nowrap;font-variant-numeric:normal"
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
        style="box-sizing:border-box;height:{ibtn}px;width:{ibtn}px;display:flex;align-items:center;justify-content:center;border-radius:var(--u4-r-3);border:1px solid var(--u4-line-ctl-2);background:var(--u4-bank-off);color:var(--u4-text-1);cursor:pointer;font-variant-numeric:normal"
      >
        <Icon name="chevronLeft" size={18} />
      </button>
      <button
        onclick={onNextChannel}
        aria-label="Next channel"
        style="box-sizing:border-box;height:{ibtn}px;width:{ibtn}px;display:flex;align-items:center;justify-content:center;border-radius:var(--u4-r-3);border:1px solid var(--u4-line-ctl-2);background:var(--u4-bank-off);color:var(--u4-text-1);cursor:pointer;font-variant-numeric:normal"
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
      style="box-sizing:border-box;width:{ibtn}px;height:{ibtn}px;border-radius:var(--u4-r-3);border:1px solid {cn.border};background:{cn.bg};color:{vm.connDot};cursor:pointer;display:flex;align-items:center;justify-content:center;padding:0;font-variant-numeric:normal"
    >
      <Icon name="plug" size={compact ? 18 : 22} />
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
    <SceneButton num={preview ? '' : vm.curSceneNum} name={preview ? 'Scenes' : vm.curSceneName} label={preview ? 'Open offline scene library' : vm.sceneAria} active={vm.isScenes} narrow={compact} onOpen={onScene} />
  {/if}
</header>

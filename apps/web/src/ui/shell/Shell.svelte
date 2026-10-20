<!-- H1 Shell: AppFrame + HeaderBar + routed content area + bank rail.
     Plain Svelte composition over the same props the layout JSON
     (ui/layouts/shell.default.json) documents; when the L0 LayoutRenderer
     lands, the JSON becomes the rendered source and this file becomes a
     thin binding. Reads uiStore (fixture URL applied on mount); screens
     plug into the outlet via registerShellScreen / registerDesignScreens
     (see registry.ts). Rail is mixer-only, like the design. -->
<script lang="ts">
  import { onMount } from 'svelte';
  import { fly } from 'svelte/transition';
  import { REDUCED_MOTION } from '../primitives/feel.ts';
  import AppFrame from '../AppFrame.svelte';
  import HeaderBar from './HeaderBar.svelte';
  import BankRail from './BankRail.svelte';
  import BootOverlay from './BootOverlay.svelte';
  import { ui } from '../state/uiStore.svelte.ts';
  import { live } from '../state/live.svelte.ts';
  import { bankDefs } from '../fixtures/designData.ts';
  import { bridgeStrip } from '../state/engineBridge.ts';
  import { modeTabToScreen } from '../state/urlBridge.ts';
  import { getShellScreen, subscribeShellScreens } from './registry.ts';
  import type { Component } from 'svelte';
  import { meterBridge } from '../screens/meters/meterBridgeState.svelte.ts';
  import { useDeviceClass } from '../modules/device.svelte.ts';
  import type { Mode } from '../state/uiLogic.ts';

  const banks = bankDefs();
  const fixtureMode = typeof window !== 'undefined' && ['1', 'true'].includes(new URLSearchParams(window.location.search).get('fixture') ?? '');

  // Live availability filter (item 4): online, banks with no strip on the
  // desk topology hide (XR18: Ch 17-32/33-48/Mtx/Grp/DCA banks vanish).
  // Offline/fixture keeps every bank (the design baseline).
  const liveBanks = $derived(
    (() => {
      const w = live.engine;
      if (live.phase !== 'online' || !w || !w.capsReady) return banks;
      void live.version;
      return banks.filter(b => b.items.some(it => bridgeStrip(w, it.id) !== null));
    })(),
  );

  // If the active bank has no live strips (e.g. just connected while on
  // Ch 17-32), fall back to the first bank that does — never strand the
  // mixer on an empty grid.
  $effect(() => {
    const bs = liveBanks;
    if (live.phase !== 'online') return;
    if (!bs.length) return;
    if (!bs.some(b => b.key === ui.state.bank)) ui.do.setBank(bs[0]!.key);
  });

  // Responsive chrome: phone viewports stack the body vertically and move
  // the bank rail to a bottom bar; desktop strings below are byte-identical
  // to the design template, so fixture baselines are untouched.
  const device = useDeviceClass();
  const isPhone = $derived(device.now === 'phone');

  onMount(() => {
    ui.initFromUrl(window.location.search);
    ui.markReady();
  });

  const screenId = $derived(modeTabToScreen(ui.state.mode, ui.state.tab) ?? 'mixer');
  // Boot mounts before all content packages finish registering; re-resolve
  // when a late registration lands instead of sticking on the placeholder.
  let shellRev = $state(0);
  $effect(() => subscribeShellScreens(() => { shellRev++; }));
  const Content = $derived.by(() => {
    void shellRev;
    return getShellScreen(screenId);
  });
  // Meter-bridge overlay starts closed: split it out of the boot chunk and
  // load it on first open.
  let MeterOverlay = $state<Component | null>(null);
  $effect(() => {
    if (meterBridge.open && !MeterOverlay) {
      import('../screens/meters/MeterBridge.svelte')
        .then((m) => { MeterOverlay = m.default; })
        .catch(() => undefined);
    }
  });

  const showRail = $derived(ui.state.mode === 'mixer');

  // Body containers copied verbatim from the design template per mode; the
  // placeholder (or the registered screen) fills the content area.
  // Phone: mixer/channel bodies become columns so the content stacks above
  // the bottom bank rail; other modes already scroll vertically.
  const bodyStyle = $derived(
    ((): string => {
      const m: Mode = ui.state.mode;
      if (m === 'mixer')
        return isPhone
          ? 'flex:1;min-height:0;display:flex;flex-direction:column;gap:4px;--shell-padding:4px'
          : 'flex:1;min-height:0;display:flex;gap:4px;--shell-padding:4px';
      if (m === 'channel')
        return isPhone
          ? 'flex:1;min-height:0;display:flex;flex-direction:column;gap:6px;--shell-padding:6px'
          : 'flex:1;min-height:0;display:flex;gap:6px;--shell-padding:6px';
      if (m === 'scenes')
        return 'flex:1;min-height:0;display:flex;flex-direction:column;gap:8px;--shell-padding:8px';
      if (m === 'fx')
        return 'flex:1;min-height:0;display:flex;flex-direction:column;gap:6px;--shell-padding:6px;overflow-y:auto';
      if (m === 'routing')
        return 'flex:1;min-height:0;display:flex;flex-wrap:wrap;gap:6px;--shell-padding:6px;overflow-y:auto;align-content:flex-start';
      return 'flex:1;min-height:0;overflow-y:auto;--shell-padding:12px';
    })(),
  );
  const placeholderStyle = $derived(
    ui.state.mode === 'mixer'
      ? 'flex:1 1 auto;min-width:0;display:flex;align-items:center;justify-content:center;color:var(--u4-text-5);font-size:15px'
      : 'flex:1;display:flex;align-items:center;justify-content:center;color:var(--u4-text-5);font-size:15px',
  );
</script>

<AppFrame>
  <HeaderBar
    vm={{ ...ui.header, connDot: live.deskConnected ? 'var(--u4-green)' : 'var(--u4-red-hi)', connAria: live.deskConnected ? 'Connection: connected, select device' : 'Connection: not connected, select device' }}
    mode={ui.state.mode}
    compact={isPhone}
    connected={live.deskConnected || live.demo || ui.state.connected === 'offline' || (fixtureMode && ui.state.connected !== null && ui.state.connected !== 'offline')}
    preview={!fixtureMode && !live.deskConnected}
    onBack={() => ui.do.goBack()}
    onUnlock={() => ui.do.unlock()}
    onFx={() => ui.do.goFx()}
    onRouting={() => ui.do.goRouting()}
    onConnect={() => ui.do.goConnect()}
    onSettings={() => ui.do.goSettings()}
    onScene={() => ui.do.openScenes()}
    onMeters={() => meterBridge.toggle()}
    onPrevChannel={() => ui.do.stepChannel(-1)}
    onNextChannel={() => ui.do.stepChannel(1)}
    metersActive={meterBridge.open}
  />
  {#if live.phase === 'connecting'}
    <BootOverlay message={live.message} />
  {/if}
  <div class="u4-shell-body" class:u4-mixer-body={ui.state.mode === 'mixer'} class:u4-edge-dock={showRail && !isPhone} style={bodyStyle}>
    {#key ui.state.mode}
      <div
        style="flex:1;min-height:0;min-width:0;display:flex;flex-direction:column"
        in:fly={{ y: REDUCED_MOTION ? 0 : -16, duration: REDUCED_MOTION ? 0 : 120 }}
      >
        {#if Content}
          <Content />
        {:else}
          <div style={placeholderStyle}>screen not built yet: {screenId}</div>
        {/if}
      </div>
    {/key}
    {#if showRail && !isPhone}
      <BankRail banks={liveBanks} activeKey={ui.state.bank} onSelect={(k) => ui.do.setBank(k)} orientation="side" />
    {/if}
  </div>
  {#if meterBridge.open && MeterOverlay}
    <MeterOverlay />
  {/if}
</AppFrame>

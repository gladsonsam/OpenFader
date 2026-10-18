<script lang="ts">
  // Pro console surface (≥1100px landscape): status bar + strip bank +
  // docked editor (follows selection) + pinned master + bottom tab pages.
  //
  // Composition only: reuses Strip (bank + FatStrip thumbs + meters),
  // EqGraph, MeterBridge, Spectrum, ShowEditor and the existing backend
  // calls. No protocol changes. Unsupported controls stay hidden via the
  // shared `hidden` array (backend rejects → field disappears).
  import { onMount } from 'svelte';
  import Strip from './Strip.svelte';
  import EqGraph from './EqGraph.svelte';
  import Spectrum from './Spectrum.svelte';
  import ShowEditor from './ShowEditor.svelte';
  import Knob from './Knob.svelte';
  import MasterSection from './MasterSection.svelte';
  import RoutingView from './RoutingView.svelte';
  import MetersView from './MetersView.svelte';
  import { EQ_DEFAULTS, type EqBand } from '../lib/eqCurve';
  import { eqBandsFor, grFor } from '../lib/bands';
  import { busColor, busIdx, dbToFader, fmtSendDb, sendDb, sendKey, type SendMap } from '../lib/sends';
  import type { Backend, MainStrip, ParamValue, Strip as StripData } from '../lib/backend';
  import type { MuteGroup } from '../lib/persist';

  export type SurfaceCell =
    | { kind: 'strip'; s: StripData; master?: boolean }
    | { kind: 'gap'; gid: string };

  type Stab = 'meters' | 'routing' | 'groups' | 'scenes' | 'setup';

  interface Props {
    showName: string;
    online: boolean;
    via: string;
    cells: SurfaceCell[];
    strips: StripData[];
    busStrips: StripData[];
    selected: number;
    solos: number[];
    clips: number[];
    mutesEnabled: boolean;
    fine: boolean;
    sofBus: number | null;
    sofColor: string;
    sofName: string;
    gr: Record<number, { gate: number; comp: number }>;
    locks: Record<number, string>;
    myLocks: number[];
    locksSupported: boolean;
    hidden: string[];
    displayFader: (s: StripData) => number;
    displayDb: (s: StripData) => string;
    onFader: (s: StripData, v: number) => void;
    onMute: (s: StripData) => void;
    onSolo: (ch: number) => void;
    onSelect: (ch: number) => void;
    onLockToggle: (ch: number) => void;
    onValueTap: (s: StripData) => void;
    onMeterTap: (ch: number) => void;
    registerMeter: (ch: number, cv: HTMLCanvasElement) => void;
    unregisterMeter: (ch: number) => void;
    // Docked editor (follows selection).
    selectedStrip: StripData | MainStrip | null;
    spectrum: number[] | null;
    grGate: number | null;
    grComp: number | null;
    onParam: (field: string, v: ParamValue) => void;
    onParamFor: (ch: number, field: string, v: ParamValue) => void;
    onName: (v: string) => void;
    sends: SendMap;
    onSend: (ch: number, bus: number, v01: number) => void;
    // Master section.
    master: StripData;
    masterFader: number;
    onMaster: (v: number) => void;
    mgroups: MuteGroup[];
    onToggleGroup: (i: number) => void;
    backend: Backend;
    say: (t: string) => void;
    // Setup page entry points.
    driver: string;
    addr: string;
    onDriver: (d: string) => void;
    onAddr: (a: string) => void;
    onConnect: () => void;
    tick: number;
    onNavigate: (ch: number) => void;
    onRoute: (ch: number, bus: number) => void;
  }

  let {
    showName,
    online,
    via,
    cells,
    strips,
    busStrips,
    selected,
    solos,
    clips,
    mutesEnabled,
    fine,
    sofBus,
    sofColor,
    sofName,
    gr,
    locks,
    myLocks,
    locksSupported,
    hidden,
    displayFader,
    displayDb,
    onFader,
    onMute,
    onSolo,
    onSelect,
    onLockToggle,
    onValueTap,
    onMeterTap,
    registerMeter,
    unregisterMeter,
    selectedStrip,
    spectrum,
    grGate,
    grComp,
    onParam,
    onParamFor,
    onName,
    sends,
    onSend,
    master,
    masterFader,
    onMaster,
    mgroups,
    onToggleGroup,
    backend,
    say,
    driver,
    addr,
    onDriver,
    onAddr,
    onConnect,
    tick,
    onNavigate,
    onRoute,
  }: Props = $props();

  const selCh = $derived(selectedStrip?.ch ?? -999);
  const selName = $derived(selectedStrip?.name ?? '—');
  const isMain = $derived(selCh < 0);

  // Editor EQ working copy: same ownership as Detail (resets per channel;
  // exact values land via onParam; thumbs mirror via lib/bands).
  let eqBands: EqBand[] = $state(EQ_DEFAULTS.map((b) => ({ ...b })));
  let eqCh = $state(-999);
  $effect(() => {
    if (selCh !== eqCh) {
      eqCh = selCh;
      eqBands = EQ_DEFAULTS.map((b) => ({ ...b }));
    }
  });

  // Staged knob values (drag locally, push on release).
  let sendDraft: Record<string, number> = $state({});
  let fxVals: Record<string, number> = $state({});
  let panVals: Record<number, number> = $state({});

  const auxBuses = $derived(busStrips.filter((b) => busIdx(b.ch) >= 0));
  const inputs = $derived(strips.filter((s) => busIdx(s.ch) < 0));

  const panHidden = $derived(hidden.includes('pan'));
  const panVal = $derived(selCh >= 0 ? (panVals[selCh] ?? 0) : 0);
  const panReadout = $derived(
    panVal === 0 ? 'C' : panVal < 0 ? `L${Math.round(-panVal * 100)}` : `R${Math.round(panVal * 100)}`,
  );

  let stab: Stab = $state('meters');
  let clock: string = $state('--:--:--');

  onMount(() => {
    const upd = (): void => {
      clock = new Date().toLocaleTimeString();
    };
    upd();
    const timer = window.setInterval(upd, 1000);
    return () => window.clearInterval(timer);
  });

  function stripColor(s: StripData): string {
    return busIdx(s.ch) >= 0 ? busColor(s.ch) : '#3fa9f5';
  }

  function commitSend(bus: number, db: number): void {
    if (selCh < 0) return;
    const k = sendKey(selCh, bus);
    const next = { ...sendDraft };
    delete next[k];
    sendDraft = next;
    onSend(selCh, bus, dbToFader(db));
  }

  function commitFx(i: number, db: number): void {
    if (selCh < 0) return;
    fxVals = { ...fxVals, [`${selCh}>fx${i}`]: db };
    onParamFor(selCh, `fx${i}_level`, db);
  }

  function commitPan(v: number): void {
    if (selCh < 0) return;
    panVals = { ...panVals, [selCh]: v };
    onParamFor(selCh, 'pan', Math.round(v * 100) / 100);
  }
</script>

{#snippet grbar(label: string, v: number | null)}
  <div
    class="grrow"
    role="img"
    aria-label={v === null ? `${label}: no GR data yet` : `${label} reduction ${v.toFixed(1)} dB`}
    title={v === null ? 'no GR data yet — needs relay/engine frames' : `${label} reduction ${v.toFixed(1)} dB`}
  >
    <span class="gl">{label}</span>
    <div class="gtrack" class:nodata={v === null}>
      <div class="gfill" style={`width: ${v === null ? 0 : Math.min(100, (v / 24) * 100)}%`}></div>
    </div>
    <span class="gv">{v === null ? '—' : v.toFixed(1)}</span>
  </div>
{/snippet}

<div class="surface" aria-label="console surface">
  <!-- 1. Top status bar -->
  <header class="sstat" aria-label="status">
    <span class="show" title="active show">{showName}</span>
    <span class="clock" title="surface clock">{clock}</span>
    <span class="sbadge" class:on={online} class:off={!online}>{online ? 'online' : 'offline'}</span>
    <span class="svia" title="transport">{via}</span>
  </header>

  <!-- 2. Strip bank -->
  <div class="bank" role="listbox" aria-label="strip bank — SEL selects, no navigation">
    {#each cells as c, i (c.kind === 'gap' ? c.gid : `ch${c.kind === 'strip' ? c.s.ch : i}${c.kind === 'strip' && c.master ? 'M' : ''}`)}
      {#if c.kind === 'gap'}
        <div class="sgap" title="blank spacer"></div>
      {:else}
        <div
          class="scell"
          class:sel={c.s.ch === selected}
          role="option"
          aria-selected={c.s.ch === selected}
          style={`--scol: ${c.master ? sofColor || '#46c46a' : stripColor(c.s)}`}
        >
          <div class="scrib" title={`${c.s.name} ch${c.s.ch < 0 ? 'M' : c.s.ch + 1}`}>{c.s.name}</div>
          <Strip
            strip={c.s}
            selected={c.s.ch === selected}
            dimmed={solos.length > 0 && !solos.includes(c.s.ch)}
            soloed={solos.includes(c.s.ch)}
            muteLocked={!mutesEnabled}
            fine={fine}
            displayValue={displayFader(c.s)}
            displayDb={displayDb(c.s)}
            clip={clips.includes(c.s.ch)}
            bands={c.s.ch >= 0 ? eqBandsFor(c.s.ch) : null}
            grDb={grFor(c.s.ch, gr)}
            lockHolder={locksSupported && c.s.ch >= 0 ? (locks[c.s.ch] ?? null) : null}
            lockMine={myLocks.includes(c.s.ch)}
            onLockToggle={locksSupported && c.s.ch >= 0 ? () => onLockToggle(c.s.ch) : undefined}
            {registerMeter}
            {unregisterMeter}
            onFader={(v) => onFader(c.s, v)}
            onMute={() => onMute(c.s)}
            onSolo={() => onSolo(c.s.ch)}
            onSelect={() => onSelect(c.s.ch)}
            onValueTap={() => onValueTap(c.s)}
            onMeterTap={() => onMeterTap(c.s.ch)}
          />
          {#if !c.master && c.s.ch >= 0 && !panHidden}
            <Knob
              label="pan"
              value={panVals[c.s.ch] ?? 0}
              min={-1}
              max={1}
              step={0.01}
              readout={(panVals[c.s.ch] ?? 0) === 0 ? 'C' : (panVals[c.s.ch] ?? 0) < 0 ? `L${Math.round(-(panVals[c.s.ch] ?? 0) * 100)}` : `R${Math.round((panVals[c.s.ch] ?? 0) * 100)}`}
              size="sm"
              onInput={(v) => { panVals = { ...panVals, [c.s.ch]: v }; }}
              onCommit={(v) => onParamFor(c.s.ch, 'pan', Math.round(v * 100) / 100)}
            />
          {/if}
          <button
            class="selbtn"
            class:on={c.s.ch === selected}
            aria-pressed={c.s.ch === selected}
            aria-label={`select ${c.s.name}`}
            title="SEL — selects, no navigation"
            onclick={() => onSelect(c.s.ch)}
          >SEL</button>
        </div>
      {/if}
    {/each}
  </div>

  <!-- 3+4. Docked editor + pinned master -->
  <div class="smain">
    <section class="dock" aria-label="selected channel editor">
      <div class="dockhead">
        <b>{selName}</b>
        {#if !isMain}<span>ch{selCh + 1}</span>{/if}
        {#if sofBus !== null}<span class="dsof" style={`--sof: ${sofColor}`}>SENDS → {sofName}</span>{/if}
      </div>
      {#if isMain}
        <div class="hint">MAIN — use the console LR fader in the master section.</div>
      {:else}
        <div class="dockgrid">
          <div class="deq">
            <h4>EQ</h4>
            <EqGraph ch={selCh} bands={eqBands} spectrum={spectrum} hidden={hidden} onParam={onParam} />
          </div>
          <div class="ddyn">
            <h4>Dynamics</h4>
            {@render grbar('gate', grGate)}
            {@render grbar('comp', grComp)}
            {#if grGate === null && grComp === null}
              <div class="hint">GR needs relay/engine frames — static until then.</div>
            {/if}
            <h4>Pan</h4>
            {#if !panHidden}
              <Knob
                label="pan"
                value={panVal}
                min={-1}
                max={1}
                step={0.01}
                readout={panReadout}
                size="md"
                onInput={(v) => { panVals = { ...panVals, [selCh]: v }; }}
                onCommit={commitPan}
              />
            {/if}
            <div class="prow">
              <label for="surf-name">name</label>
              <input id="surf-name" type="text" maxlength="16" value={selName} onchange={(e) => onName(e.currentTarget.value)} />
            </div>
          </div>
          <div class="dsends">
            <h4>Sends</h4>
            {#if !auxBuses.length}
              <div class="hint">no buses on this backend yet</div>
            {:else}
              <div class="knobrow">
                {#each auxBuses as b (b.ch)}
                  {@const k = sendKey(selCh, b.ch)}
                  {@const db = sendDraft[k] ?? sendDb(sends, selCh, b.ch)}
                  <Knob
                    label={b.name}
                    value={db}
                    min={-120}
                    max={10}
                    step={0.5}
                    readout={fmtSendDb(db)}
                    size="sm"
                    onInput={(v) => { sendDraft = { ...sendDraft, [k]: v }; }}
                    onCommit={(v) => commitSend(b.ch, v)}
                  />
                {/each}
              </div>
            {/if}
            <h4>FX</h4>
            <div class="knobrow">
              {#each [1, 2, 3, 4] as n (n)}
                {@const fk = `${selCh}>fx${n}`}
                {@const fdb = fxVals[fk] ?? -120}
                {#if !hidden.includes(`fx${n}_level`)}
                  <Knob
                    label={`FX${n}`}
                    value={fdb}
                    min={-120}
                    max={10}
                    step={0.5}
                    readout={fmtSendDb(fdb)}
                    size="sm"
                    onInput={(v) => { fxVals = { ...fxVals, [fk]: v }; }}
                    onCommit={(v) => commitFx(n, v)}
                  />
                {/if}
              {/each}
            </div>
          </div>
        </div>
        <Spectrum ch={selCh} bins={spectrum} />
      {/if}
    </section>

    <MasterSection
      {master}
      displayValue={masterFader}
      displayDb={displayDb(master)}
      soloed={solos.includes(-1)}
      clip={clips.includes(-1)}
      {mutesEnabled}
      {mgroups}
      {onToggleGroup}
      {registerMeter}
      {unregisterMeter}
      onFader={(v) => onMaster(v)}
      onMute={() => onMute(master)}
      onSolo={() => onSolo(-1)}
      onSelect={() => onSelect(-1)}
      onValueTap={() => onValueTap(master)}
      onMeterTap={() => onMeterTap(-1)}
    />
  </div>

  <!-- 5. Bottom tab pages -->
  <div class="spanel">
    {#if stab === 'meters'}
      <MetersView
        {strips}
        {selected}
        {solos}
        {clips}
        {tick}
        spectrumCh={selCh}
        {spectrum}
        onNavigate={onNavigate}
        onSolo={onSolo}
        onClearClip={onMeterTap}
      />
    {:else if stab === 'routing'}
      <RoutingView inputs={inputs} buses={busStrips} {sends} {selected} onCell={onRoute} />
    {:else if stab === 'groups'}
      <div class="sgroups" role="group" aria-label="mute groups">
        {#each mgroups as g, i (g.name)}
          <button
            class="grp"
            class:has={g.members.length > 0}
            onclick={() => onToggleGroup(i)}
            title={g.members.length ? `toggle mute: ${g.members.length} ch` : 'empty — assign in Channel view'}
            aria-label={g.members.length ? `toggle mute group ${g.name}` : `mute group ${g.name}, empty`}
          >{g.name}{g.members.length ? `·${g.members.length}` : ''}</button>
        {/each}
        <span class="hint">assign channels from Channel view</span>
      </div>
    {:else if stab === 'scenes'}
      <ShowEditor strips={strips} backend={backend} say={say} />
    {:else}
      <div class="ssetup" aria-label="setup">
        <label>driver
          <select value={driver} onchange={(e) => onDriver(e.currentTarget.value)}>
            <option value="sim">sim</option>
            <option value="xair">xair</option>
            <option value="dl32s">dl32s</option>
          </select>
        </label>
        <label>addr <input type="text" inputmode="numeric" placeholder="192.168.1.10 (blank = sim)" size="22" value={addr} oninput={(e) => onAddr(e.currentTarget.value)} /></label>
        <button class="big" onclick={onConnect}>connect</button>
        <span class="svia">{via}</span>
      </div>
    {/if}
  </div>
  <nav class="stabs" aria-label="surface pages">
    {#each [{ id: 'meters', label: 'Meters' }, { id: 'routing', label: 'Routing' }, { id: 'groups', label: 'Groups' }, { id: 'scenes', label: 'Scenes' }, { id: 'setup', label: 'Setup' }] as t (t.id)}
      <button class:on={stab === t.id} aria-current={stab === t.id ? 'page' : undefined} onclick={() => { stab = t.id as Stab; }}>{t.label}</button>
    {/each}
  </nav>
</div>

<style>
  .surface { display: flex; flex-direction: column; gap: 6px; padding: 6px 10px; }
  /* 1. Status bar */
  .sstat {
    display: flex; gap: 12px; align-items: center;
    background: var(--panel); border: 1px solid var(--line); border-radius: 10px;
    padding: 6px 12px; font-size: 13px;
  }
  .show { font-weight: 800; color: var(--acc); }
  .clock { font-variant-numeric: tabular-nums; color: var(--txt); }
  .eng, .sr { color: var(--dim); font-variant-numeric: tabular-nums; }
  .sbadge { padding: 3px 10px; border-radius: 20px; font-size: 12px; font-weight: 700; }
  .sbadge.off { background: #3a1416; color: #ff8a8e; }
  .sbadge.on { background: #123a1e; color: #7dffa4; }
  .svia { color: var(--dim); font-size: 12px; margin-left: auto; }
  /* 2. Strip bank */
  .bank {
    display: flex; gap: 6px; overflow-x: auto; align-items: stretch;
    background: #000; border: 1px solid var(--line); border-radius: 10px; padding: 6px;
  }
  .sgap { flex: 0 0 32px; border: 1px dashed var(--line); border-radius: 8px; }
  .scell {
    flex: 0 0 auto; display: flex; flex-direction: column; gap: 4px; align-items: center;
    border: 1px solid var(--line); border-radius: 8px; padding: 4px;
    border-top: 3px solid var(--scol, var(--acc));
    background: var(--panel);
  }
  .scell.sel { border-color: var(--acc); box-shadow: 0 0 0 1px var(--acc); }
  .scrib {
    font-size: 11px; font-weight: 800; max-width: 92px;
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }
  .selbtn { min-height: 32px; min-width: 0; width: 100%; padding: 0 6px; font-size: 12px; font-weight: 800; }
  .selbtn.on { background: #1c2733; border-color: var(--txt); }
  /* 3+4. Editor + master */
  .smain { display: grid; grid-template-columns: minmax(0, 1fr) 230px; gap: 8px; align-items: start; }
  .dock {
    background: var(--panel); border: 1px solid var(--line); border-radius: 12px;
    padding: 8px; min-width: 0; display: flex; flex-direction: column; gap: 8px;
  }
  .dockhead { display: flex; gap: 8px; align-items: baseline; }
  .dockhead b { font-size: 16px; }
  .dockhead span { color: var(--dim); font-size: 12px; }
  .dockhead .dsof { color: var(--sof, var(--yel)); border: 1px solid var(--sof, var(--yel)); border-radius: 6px; padding: 1px 6px; font-weight: 800; }
  .dockgrid { display: grid; grid-template-columns: minmax(0, 1.4fr) minmax(0, 1fr) minmax(0, 1fr); gap: 10px; }
  .dock h4 { margin: 0 0 4px; font-size: 11px; text-transform: uppercase; letter-spacing: 0.6px; color: var(--dim); }
  .hint { font-size: 11px; color: var(--dim); }
  .grrow { display: flex; align-items: center; gap: 6px; font-size: 12px; }
  .gl { width: 34px; color: var(--dim); }
  .gtrack { flex: 1; height: 8px; background: #1a2230; border-radius: 4px; overflow: hidden; }
  .gtrack.nodata { background: repeating-linear-gradient(45deg, #1a2230, #1a2230 4px, #141b28 4px, #141b28 8px); }
  .gfill { height: 100%; background: linear-gradient(90deg, #4ade80, #facc15 60%, #f87171); }
  .gv { min-width: 30px; text-align: right; color: var(--dim); }
  .knobrow { display: flex; flex-wrap: wrap; gap: 6px; }
  .prow { display: flex; gap: 6px; align-items: center; margin-top: 6px; }
  .prow label { flex: 0 0 auto; font-size: 12px; color: var(--dim); }
  .prow input { flex: 1; min-width: 0; }
  /* 5. Bottom tabs */
  .spanel {
    background: var(--panel); border: 1px solid var(--line); border-radius: 12px;
    padding: 8px; max-height: 320px; overflow: auto; min-height: 120px;
  }
  .stabs { display: flex; gap: 6px; }
  .stabs button { flex: 1; font-weight: 800; }
  .stabs button.on { background: #1c2733; border-color: var(--txt); }
  .sgroups { display: flex; gap: 6px; align-items: center; flex-wrap: wrap; }
  .grp.has { border-color: var(--yel); }
  .ssetup { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
  .ssetup label { color: var(--dim); display: flex; gap: 6px; align-items: center; }
</style>

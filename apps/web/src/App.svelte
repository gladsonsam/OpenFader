<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { fade, fly } from 'svelte/transition';
  import { selectBackend } from './lib/backend';
  import type { Backend, MainStrip, ParamValue, Unsub, Strip as StripData } from './lib/backend';
  import { levels, setLevels } from './lib/levels';
  import { paintBridge, paintStageMeter } from './lib/meters';
  import { faderDb } from './lib/format';
  import {
    loadJSON, saveJSON, defaultLayers, defaultGroups, cleanOrder, LS,
    type LayerDef, type OrderEntry, type MuteGroup,
  } from './lib/persist';
  import {
    isBusCh, busIdx, auxField, busColor,
    dbToFader, faderToDb, fmtSendDb, loadSends, saveSends, sendDb, sendKey,
    type SendMap,
  } from './lib/sends';
  import Strip from './components/Strip.svelte';
  import Detail from './components/Detail.svelte';
  import Overview from './components/Overview.svelte';
  import ShowEditor from './components/ShowEditor.svelte';
  import ValueEntry from './components/ValueEntry.svelte';

  const backend: Backend = selectBackend();
  const via = backend.kind === 'tauri' ? 'tauri ipc' : `ws ${backend.url}`;

  const MAIN: MainStrip = { ch: -1, name: 'MAIN' };

  type View = 'overview' | 'mixer' | 'channel' | 'shows';
  type Cell = { kind: 'strip'; s: StripData; master?: boolean } | { kind: 'gap'; gid: string };
  interface Editing { ch: number; title: string; sub: string; value: number; isSend: boolean }

  const defaultStrips = (): StripData[] =>
    Array.from({ length: 8 }, (_, i) => ({
      ch: i,
      name: `CH${String(i + 1).padStart(2, '0')}`,
      fader: 0.75,
      muted: false,
    }));

  function loadLayers(): LayerDef[] {
    const raw = loadJSON<unknown>(LS.layers, null);
    if (!Array.isArray(raw)) return defaultLayers();
    const out: LayerDef[] = [];
    for (const e of raw) {
      if (e !== null && typeof e === 'object') {
        const rec = e as Record<string, unknown>;
        if (typeof rec['id'] === 'string' && typeof rec['name'] === 'string') {
          out.push({ id: rec['id'], name: rec['name'], builtin: rec['builtin'] === true });
        }
      }
    }
    return out.length ? out : defaultLayers();
  }

  function loadOrders(): Record<string, OrderEntry[]> {
    const raw = loadJSON<Record<string, unknown>>(LS.orders, {});
    const out: Record<string, OrderEntry[]> = {};
    for (const [k, v] of Object.entries(raw)) {
      const c = cleanOrder(v);
      if (c) out[k] = c;
    }
    return out;
  }

  function loadView(): View {
    const v = loadJSON<string>(LS.view, 'mixer');
    return v === 'overview' || v === 'mixer' || v === 'channel' || v === 'shows' ? v : 'mixer';
  }

  /* ---------- state ---------- */
  let strips: StripData[] = $state(defaultStrips());
  let online: boolean = $state(false);
  let status: string = $state('');
  let view: View = $state(loadView());
  let layer: string = $state('inputs');
  let layers: LayerDef[] = $state(loadLayers());
  let orders: Record<string, OrderEntry[]> = $state(loadOrders());
  let mgroups: MuteGroup[] = $state(loadJSON<MuteGroup[]>(LS.groups, defaultGroups()));
  let sends: SendMap = $state(loadSends());
  let mutesEnabled: boolean = $state(loadJSON<boolean>(LS.muteEn, true));
  let fineMode: boolean = $state(loadJSON<boolean>(LS.fine, false));
  let sofBus: number | null = $state(null); // transient: sends-on-fader destination
  let clips: number[] = $state([]); // latched clip channels, tap meter to clear
  let editing: Editing | null = $state(null); // tap-a-value modal
  let editLayout: boolean = $state(false);
  let tick: number = $state(0); // ~8Hz pulse for the Overview health rows
  let selected: number = $state(0);
  let solos: number[] = $state([]);
  let hidden: string[] = $state([]); // param fields the backend rejected
  let storeArm: boolean = $state(false);
  let driver: string = $state('sim');
  let addr: string = $state('');
  let showBridge: boolean = $state(false);
  let master: { fader: number } = $state({ fader: 0.8 }); // local monitor-dim until backend exposes main
  let lastMeterAt = 0;
  let lastTick = 0;
  let spectra: Record<number, number[]> = $state({}); // ch -> bins (live spectrum; null until relay sends it)
  let gains: Record<number, number> = {}; // detail gain dB per ch (staging display only)

  let bridge: HTMLCanvasElement | null = $state(null);
  const meterMap = new Map<number, HTMLCanvasElement>(); // ch -> canvas (imperative paint, no re-render)
  const registerMeter = (ch: number, cv: HTMLCanvasElement): void => {
    meterMap.set(ch, cv);
  };
  const unregisterMeter = (ch: number): void => {
    meterMap.delete(ch);
  };

  /* ---------- helpers ---------- */
  const isBusStrip = (s: StripData): boolean => isBusCh(s.ch) || /^bus/i.test(s.name);
  const inSof = $derived(sofBus !== null);
  const sofColorV = $derived(sofBus === null ? '' : busColor(sofBus));
  const sofName = $derived(sofBus === null ? '' : (strips.find((s) => s.ch === sofBus)?.name ?? `bus ${sofBus}`));
  const busStrips: StripData[] = $derived(strips.filter(isBusStrip));

  const mainPseudo = (): StripData => ({ ch: -1, name: 'MAIN', fader: master.fader, muted: false, main: true });

  function defaultOrderFor(layerId: string): OrderEntry[] {
    if (layerId === 'mains') return [{ t: 'ch', ch: -1 }];
    const pool = layerId === 'buses' ? strips.filter(isBusStrip) : strips.filter((s) => !isBusStrip(s));
    return pool.map((s): OrderEntry => ({ t: 'ch', ch: s.ch }));
  }

  function saveOrders(): void {
    saveJSON(LS.orders, orders);
  }
  function saveLayers(): void {
    saveJSON(LS.layers, layers);
  }
  function saveGroups(): void {
    saveJSON(LS.groups, mgroups);
  }

  function setView(v: View): void {
    view = v;
    saveJSON(LS.view, v);
  }

  let selectedStrip: StripData | MainStrip | null = $derived(
    [...strips, MAIN].find((x) => x.ch === selected) ?? null,
  );

  // Channel view needs a full Strip (Detail + big fader); MAIN maps to master.
  let channelStrip: StripData | null = $derived.by(() => {
    if (selected < 0) return mainPseudo();
    return strips.find((s) => s.ch === selected) ?? null;
  });

  let mixerCells: Cell[] = $derived.by(() => {
    if (sofBus !== null) {
      const bus = sofBus;
      const cells: Cell[] = strips.filter((s) => !isBusStrip(s)).map((s): Cell => ({ kind: 'strip', s }));
      const b = strips.find((s) => s.ch === bus);
      if (b) cells.push({ kind: 'strip', s: b, master: true });
      return cells;
    }
    const ord = orders[layer] ?? defaultOrderFor(layer);
    const out: Cell[] = [];
    for (const e of ord) {
      if (e.t === 'gap') {
        out.push({ kind: 'gap', gid: e.gid });
        continue;
      }
      if (e.ch === -1) {
        out.push({ kind: 'strip', s: mainPseudo() });
        continue;
      }
      const f = strips.find((s) => s.ch === e.ch);
      if (f) out.push({ kind: 'strip', s: f });
    }
    return out;
  });

  /** Fader position actually shown for a strip (send level in SoF). */
  function displayFader(s: StripData): number {
    if (s.main || s.ch < 0) return master.fader;
    if (sofBus !== null && !isBusStrip(s)) return dbToFader(sendDb(sends, s.ch, sofBus));
    return s.fader;
  }

  function displayDb(s: StripData): string {
    if (s.main || s.ch < 0) return faderDb(master.fader);
    if (sofBus !== null && !isBusStrip(s)) return fmtSendDb(sendDb(sends, s.ch, sofBus));
    return faderDb(s.fader);
  }

  const say = (t: string): void => {
    status = t || '';
  };
  const clamp01 = (v: number): number => Math.min(1, Math.max(0, v));
  const errMsg = (e: unknown): string => (e instanceof Error ? e.message : String(e));

  /* ---------- fader / mute / select ---------- */
  async function setFader(s: StripData, v: number): Promise<void> {
    v = clamp01(v);
    if (s.main || s.ch < 0) {
      master.fader = v;
      return;
    }
    const cur = strips.find((x) => x.ch === s.ch);
    if (cur) cur.fader = v; // optimistic for touch latency
    try {
      await backend.setFader(s.ch, v);
    } catch (e) {
      say(`fader ch${s.ch + 1}: ${errMsg(e)}`);
    }
  }

  /** Send level in SoF: cached locally, pushed via the existing `param` cmd. */
  async function setSend(ch: number, v: number): Promise<void> {
    const bus = sofBus;
    if (bus === null) return;
    const db = Math.round(faderToDb(v) * 2) / 2;
    sends[sendKey(ch, bus)] = db;
    saveSends(sends);
    try {
      await backend.setParam(ch, auxField(bus), db);
    } catch (e) {
      say(`send ch${ch + 1}→${sofName}: ${errMsg(e)} (kept locally)`);
    }
  }

  function stripFader(s: StripData, v: number): Promise<void> {
    // In SoF, input strips drive sends; the pinned bus master drives its fader.
    if (sofBus !== null && !s.main && s.ch >= 0 && !isBusStrip(s)) return setSend(s.ch, v);
    return setFader(s, v);
  }

  async function toggleMute(s: StripData): Promise<void> {
    if (s.main || s.ch < 0) return;
    if (!mutesEnabled) {
      say('mutes disabled — tap MUTE EN to arm mute buttons');
      return;
    }
    const next = !s.muted;
    const cur = strips.find((x) => x.ch === s.ch);
    if (cur) cur.muted = next;
    try {
      await backend.setMute(s.ch, next);
    } catch (e) {
      if (cur) cur.muted = !next;
      say(`mute ch${s.ch + 1}: ${errMsg(e)}`);
    }
  }

  function toggleSolo(ch: number): void {
    solos = solos.includes(ch) ? solos.filter((c) => c !== ch) : [...solos, ch];
  }

  function select(ch: number): void {
    selected = ch;
  }

  function jumpTo(ch: number): void {
    selected = ch;
    setView('mixer');
  }

  function clearClip(ch: number): void {
    if (clips.includes(ch)) clips = clips.filter((c) => c !== ch);
  }

  /* ---------- sends-on-fader ---------- */
  function enterSof(busCh: number): void {
    if (busIdx(busCh) < 0) {
      say('that bus has no aux send tap on this backend');
      return;
    }
    sofBus = busCh;
    if (!solos.includes(busCh)) solos = [...solos, busCh]; // auto-cue the destination
    setView('mixer');
    say(`sends → ${strips.find((s) => s.ch === busCh)?.name ?? busCh}: faders are sends — EXIT returns to LR`);
  }

  function exitSof(): void {
    if (sofBus !== null) solos = solos.filter((c) => c !== sofBus);
    sofBus = null;
    say('back to LR');
  }

  async function copyLrToBus(): Promise<void> {
    const bus = sofBus;
    if (bus === null) return;
    const ins = strips.filter((s) => !isBusStrip(s));
    for (const s of ins) {
      const db = Math.round(faderToDb(s.fader) * 2) / 2;
      sends[sendKey(s.ch, bus)] = db;
      try {
        await backend.setParam(s.ch, auxField(bus), db);
      } catch (e) {
        say(`copy LR: ch${s.ch + 1} ${errMsg(e)}`);
        break;
      }
    }
    saveSends(sends);
    say(`copied LR (${ins.length} sends) → ${sofName}`);
  }

  /* ---------- mute groups ---------- */
  async function toggleGroup(g: number): Promise<void> {
    const grp = mgroups[g];
    if (!grp || !grp.members.length) {
      say(`mute group ${g + 1} empty — assign channels from Channel view`);
      return;
    }
    if (!mutesEnabled) {
      say('mutes disabled — tap MUTE EN to arm mute buttons');
      return;
    }
    const anyUnmuted = grp.members.some((ch) => !strips.find((s) => s.ch === ch)?.muted);
    for (const ch of grp.members) {
      const cur = strips.find((s) => s.ch === ch);
      if (!cur || cur.muted === anyUnmuted) continue;
      cur.muted = anyUnmuted;
      try {
        await backend.setMute(ch, anyUnmuted);
      } catch (e) {
        cur.muted = !anyUnmuted;
        say(`group ${grp.name}: ch${ch + 1} ${errMsg(e)}`);
        break;
      }
    }
    say(`group ${grp.name} ${anyUnmuted ? 'muted' : 'unmuted'}`);
  }

  function mgToggle(ch: number, g: number): void {
    const grp = mgroups[g];
    if (!grp) return;
    grp.members = grp.members.includes(ch) ? grp.members.filter((m) => m !== ch) : [...grp.members, ch];
    saveGroups();
  }

  /* ---------- custom layers ---------- */
  function orderOf(): OrderEntry[] {
    return orders[layer] ?? defaultOrderFor(layer);
  }

  function setOrder(next: OrderEntry[]): void {
    orders[layer] = next;
    saveOrders();
  }

  function moveEntry(idx: number, dir: -1 | 1): void {
    const ord = [...orderOf()];
    const j = idx + dir;
    if (idx < 0 || idx >= ord.length || j < 0 || j >= ord.length) return;
    const [e] = ord.splice(idx, 1);
    if (e) ord.splice(j, 0, e);
    setOrder(ord);
  }

  function addGap(idx: number): void {
    const ord = [...orderOf()];
    ord.splice(idx + 1, 0, { t: 'gap', gid: `gap-${Date.now()}-${idx}` });
    setOrder(ord);
  }

  function removeEntry(idx: number): void {
    const ord = [...orderOf()];
    ord.splice(idx, 1);
    setOrder(ord);
  }

  function resetLayout(): void {
    delete orders[layer];
    saveOrders();
    say(`${layer} layout reset to backend order`);
  }

  function renameLayer(): void {
    const cur = layers.find((l) => l.id === layer);
    const name = prompt('Layer name?', cur?.name ?? layer);
    if (!name || !cur) return;
    cur.name = name.trim().slice(0, 16) || cur.name;
    saveLayers();
  }

  function addLayer(): void {
    const name = prompt('New layer name?', `User ${layers.length + 1}`);
    if (!name || !name.trim()) return;
    const id = `u${Date.now()}`;
    const members = mixerCells.flatMap((c): OrderEntry[] =>
      c.kind === 'strip' && !c.s.main && c.s.ch >= 0 ? [{ t: 'ch', ch: c.s.ch }] : [],
    );
    layers = [...layers, { id, name: name.trim().slice(0, 16) }];
    orders[id] = members;
    saveLayers();
    saveOrders();
    layer = id;
    say(`layer "${name.trim()}" created (${members.length} strips) — edit layout to refine`);
  }

  function deleteLayer(): void {
    const cur = layers.find((l) => l.id === layer);
    if (!cur || cur.builtin) {
      say('only custom layers can be deleted');
      return;
    }
    layers = layers.filter((l) => l.id !== layer);
    delete orders[layer];
    saveLayers();
    saveOrders();
    layer = 'inputs';
  }

  /* ---------- tap-a-value exact entry ---------- */
  function openValue(s: StripData): void {
    if (s.main || s.ch < 0) {
      editing = { ch: -1, title: 'MAIN', sub: 'master fader', value: master.fader, isSend: false };
      return;
    }
    if (sofBus !== null && !isBusStrip(s)) {
      editing = {
        ch: s.ch,
        title: `${s.name} → ${sofName}`,
        sub: 'send level',
        value: dbToFader(sendDb(sends, s.ch, sofBus)),
        isSend: true,
      };
      return;
    }
    editing = { ch: s.ch, title: s.name, sub: 'fader', value: s.fader, isSend: false };
  }

  async function applyValue(v: number): Promise<void> {
    const e = editing;
    editing = null;
    if (!e) return;
    if (e.isSend) {
      await setSend(e.ch, v);
      return;
    }
    if (e.ch < 0) {
      master.fader = clamp01(v);
      return;
    }
    const s = strips.find((x) => x.ch === e.ch);
    if (s) await setFader(s, v);
  }

  /* ---------- detail params ---------- */
  async function pushParam(field: string, v: ParamValue): Promise<void> {
    if (field === 'gain' && typeof v === 'number') gains[selected] = v; // staging display
    try {
      await backend.setParam(selected, field, v);
    } catch (e) {
      if (!hidden.includes(field)) hidden = [...hidden, field];
      say(`${field}: unsupported (${errMsg(e)})`);
    }
  }

  function pushName(v: string): void {
    void pushParam('name', v);
  }

  /* ---------- scenes (local snapshot slots) ---------- */
  const SCENE_KEY = 'openmix.scenes.v1';
  function loadScenes(): Record<string, StripData[]> {
    try {
      const raw: unknown = JSON.parse(localStorage.getItem(SCENE_KEY) || 'null');
      if (raw !== null && typeof raw === 'object' && !Array.isArray(raw)) {
        // Minimal documented cast: shape is app-written; entries validated on recall.
        return raw as Record<string, StripData[]>;
      }
      return {};
    } catch {
      return {};
    }
  }
  let scenes: Record<string, StripData[]> = $state(loadScenes());

  function storeScene(i: number): void {
    const all = loadScenes();
    all[i] = strips.map((s) => ({ ...s }));
    localStorage.setItem(SCENE_KEY, JSON.stringify(all));
    scenes = all;
    storeArm = false;
    say(`scene ${i} stored (${strips.length} strips)`);
  }

  async function recallScene(i: number): Promise<void> {
    const snap: StripData[] | undefined = loadScenes()[i];
    if (!snap) {
      say(`scene ${i} empty — tap store, then S${i}`);
      return;
    }
    say(`recalling scene ${i}…`);
    for (const s of snap) {
      const cur = strips.find((x) => x.ch === s.ch);
      if (!cur) continue;
      try {
        if (cur.fader !== s.fader) await backend.setFader(s.ch, s.fader);
        if (cur.muted !== s.muted) await backend.setMute(s.ch, s.muted);
        cur.fader = s.fader;
        cur.muted = s.muted;
      } catch (e) {
        say(`scene ${i}: ch${s.ch + 1} ${errMsg(e)}`);
        break;
      }
    }
    say(`scene ${i} recalled`);
  }

  /* ---------- connect ---------- */
  async function connect(): Promise<void> {
    say(`connecting ${driver} ${addr.trim() || '(sim)'}…`);
    try {
      const msg = await backend.connect(driver, addr.trim());
      say(msg);
    } catch (e) {
      say(`connect failed: ${errMsg(e)}`);
    }
  }

  /* ---------- meter loop ---------- */
  let raf = 0;

  function sizeBridge(): void {
    if (!bridge) return;
    bridge.width = bridge.clientWidth * devicePixelRatio;
    bridge.height = 64 * devicePixelRatio;
  }

  function loop(now: number): void {
    for (const [ch, cv] of meterMap) {
      const s = ch === -1 ? null : strips.find((x) => x.ch === ch);
      const f = ch === -1 ? master.fader : s ? displayFader(s) : 0;
      const raw = ch === -1 ? f * 0.9 : !s ? 0 : (levels[s.ch] ?? 0);
      if (!clips.includes(ch) && raw >= 0.985) clips = [...clips, ch]; // latch
      const input = clamp01(raw / Math.max(f, 0.08)); // pre-fader estimate
      paintStageMeter(cv, s?.muted ? 0 : input, s?.muted ? 0 : raw, clips.includes(ch));
    }
    if (showBridge && bridge) {
      const frame = strips.map((s, i) => (s.muted ? 0 : (levels[i] ?? 0)));
      paintBridge(bridge, frame);
    }
    if (now - lastTick > 120) {
      lastTick = now;
      tick++;
    }
    raf = requestAnimationFrame(loop);
  }

  function onKey(e: KeyboardEvent): void {
    if (/INPUT|SELECT|TEXTAREA/.test(document.activeElement?.tagName ?? '')) return;
    if (editing) return;
    const s = strips.find((x) => x.ch === selected);
    if (!s) return;
    const step = fineMode ? 0.002 : e.shiftKey ? 0.05 : 0.01;
    if (e.key === 'ArrowUp' || e.key === 'ArrowRight') {
      void stripFader(s, displayFader(s) + step);
      e.preventDefault();
    } else if (e.key === 'ArrowDown' || e.key === 'ArrowLeft') {
      void stripFader(s, displayFader(s) - step);
      e.preventDefault();
    } else if (e.key === 'm' || e.key === 'M') {
      void toggleMute(s);
    }
  }

  function onResize(): void {
    if (showBridge) sizeBridge();
  }

  onMount(() => {
    if (!layers.some((l) => l.id === layer)) layer = 'inputs';
    const offs: Unsub[] = [
      backend.onState((next) => {
        strips = next;
        if (!strips.some((s) => s.ch === selected)) selected = strips[0]?.ch ?? 0;
      }),
      backend.onMeters((lv) => {
        setLevels(lv);
        lastMeterAt = performance.now();
      }),
    ];
    if (backend.onSpectrum) {
      offs.push(
        backend.onSpectrum(({ ch, bins }) => {
          const next = { ...spectra };
          next[ch] = bins;
          spectra = next;
        }),
      );
    }
    if (backend.onStatus) offs.push(backend.onStatus((on) => {
      online = on;
    }));

    const staleTimer = window.setInterval(() => {
      // Offline if no meter frame for >1.5s while expecting one.
      if (lastMeterAt && performance.now() - lastMeterAt > 1500) online = false;
    }, 1000);

    if (showBridge) sizeBridge();
    raf = requestAnimationFrame(loop);
    window.addEventListener('resize', onResize);

    // Auto-connect sim for instant use; failures surface on the badge.
    backend
      .connect('sim', '')
      .then((m) => say(m))
      .catch((e: unknown) => say(errMsg(e)));

    return () => {
      offs.forEach((off) => off());
      window.clearInterval(staleTimer);
      cancelAnimationFrame(raf);
      window.removeEventListener('resize', onResize);
    };
  });

  onDestroy(() => {
    if (backend.kind === 'ws') backend.close?.();
  });

  $effect(() => {
    // Size the bridge canvas once it becomes visible.
    if (showBridge && bridge) sizeBridge();
  });
</script>

<svelte:window onkeydown={onKey} />

<div id="app" class:sof={inSof} style={inSof ? `--sof: ${sofColorV}` : ''}>
  <header id="topbar">
    <span id="logo">openmix</span>
    <label>driver
      <select bind:value={driver}>
        <option value="sim">sim</option>
        <option value="xair">xair</option>
        <option value="dl32s">dl32s</option>
      </select>
    </label>
    <label>addr <input id="addr" inputmode="numeric" placeholder="192.168.1.10 (blank = sim)" size="22" bind:value={addr} /></label>
    <button id="conn" class="big" onclick={connect}>connect</button>
    <span id="badge" class:online class:offline={!online}>{online ? 'online' : 'offline'}</span>
    <span id="via" title="transport">{via}</span>
    <button id="bridgeBtn" class:on={showBridge} title="toggle meter bridge" onclick={() => { showBridge = !showBridge; }}>bridge</button>
  </header>

  <!-- Mackie 3-view nav: always visible, active one dark. -->
  <nav id="views" aria-label="views">
    {#each [{ id: 'overview', label: 'Overview' }, { id: 'mixer', label: 'Mixer' }, { id: 'channel', label: 'Channel' }, { id: 'shows', label: 'Shows' }] as v (v.id)}
      <button class:on={view === v.id} onclick={() => setView(v.id as View)}>{v.label}</button>
    {/each}
  </nav>

  <!-- QSC-style aux rail: always-visible bus selects, one tap flips to that mix. -->
  {#if view !== 'channel' && busStrips.length}
    <nav id="auxrail" aria-label="mix destination">
      <button class:lr={!inSof} onclick={exitSof} title="back to LR">LR</button>
      {#each busStrips as b (b.ch)}
        <button
          class:dest={sofBus === b.ch}
          style={sofBus === b.ch ? `border-color: ${busColor(b.ch)}; color: ${busColor(b.ch)}` : ''}
          onclick={() => (sofBus === b.ch ? exitSof() : enterSof(b.ch))}
          title="sends-on-fader to {b.name}"
        >{b.name}</button>
      {/each}
    </nav>
  {/if}

  <!-- SoF banner: the mode can never be mistaken for LR. -->
  {#if inSof}
    <div id="sofbanner" style={`--sof: ${sofColorV}`} role="alert">
      <b>SENDS → {sofName}</b>
      <span>faders are sends, not LR</span>
      <button onclick={copyLrToBus} title="start from current LR balance">copy LR</button>
      <button class="exit" onclick={exitSof} title="exit sends-on-fader (same gesture as entry)">EXIT ✕</button>
    </div>
  {/if}

  {#if view === 'mixer'}
    <nav id="layers" role="tablist" aria-label="fader banks">
      {#each layers as l (l.id)}
        <button role="tab" data-layer={l.id} class:on={layer === l.id} onclick={() => { layer = l.id; }}>
          {l.name}
        </button>
      {/each}
    </nav>

    <div id="toolbar">
      <button
        id="muteEn"
        class:on={mutesEnabled}
        title="master mute-enable: when off, all mute buttons lock out"
        onclick={() => { mutesEnabled = !mutesEnabled; saveJSON(LS.muteEn, mutesEnabled); say(mutesEnabled ? 'mutes armed' : 'mutes locked out'); }}
      >MUTE EN</button>
      <button
        class:on={fineMode}
        title="fine-adjust: fader drags move at 20%, smaller key steps"
        onclick={() => { fineMode = !fineMode; saveJSON(LS.fine, fineMode); }}
      >FINE</button>
      <button class:on={editLayout} title="edit bank layout: reorder, spacers, custom layers" onclick={() => { editLayout = !editLayout; }}>LAYOUT</button>
      {#if editLayout}
        <button onclick={addLayer} title="new custom layer from current strips">+layer</button>
        <button onclick={renameLayer} title="rename current layer tab">rename</button>
        <button onclick={resetLayout} title="restore backend order">reset</button>
        {#if !(layers.find((l) => l.id === layer)?.builtin)}
          <button onclick={deleteLayer} title="delete this custom layer">del</button>
        {/if}
      {/if}
      <span class="sep"></span>
      <span class="lbl">groups</span>
      {#each mgroups as g, i (g.name)}
        <button
          class="grp"
          class:has={g.members.length > 0}
          onclick={() => toggleGroup(i)}
          title={g.members.length ? `toggle mute: ${g.members.length} ch (${g.name})` : `${g.name} empty — assign in Channel view`}
        >{g.name}{g.members.length ? `·${g.members.length}` : ''}</button>
      {/each}
    </div>
  {/if}

  {#if showBridge}
    <canvas id="bridge" bind:this={bridge} transition:fade={{ duration: 150 }}></canvas>
  {/if}

  <div id="scenes">
    <span class="lbl">scenes</span>
    <div id="slots">
      {#each [1, 2, 3, 4] as i (i)}
        <div class="slot">
          <b>{i}{scenes[i] ? '●' : '○'}</b>
          <button
            title={scenes[i] ? 'tap: recall' : 'empty — arm store, then tap'}
            onclick={() => (storeArm ? storeScene(i) : recallScene(i))}
          >S{i}</button>
        </div>
      {/each}
    </div>
    <button id="store" class:armed={storeArm} onclick={() => { storeArm = !storeArm; say(storeArm ? 'store armed — tap a slot' : ''); }}>store</button>
    <span id="status" role="status">{status}</span>
  </div>

  {#if view === 'overview'}
    <main id="ovwrap">
      <Overview
        strips={[...strips.filter((s) => !isBusStrip(s)), ...busStrips]}
        {solos}
        {clips}
        {tick}
        {selected}
        {mutesEnabled}
        onJump={jumpTo}
        onMute={(s) => toggleMute(s)}
        onSolo={toggleSolo}
        onSof={enterSof}
        isBus={isBusStrip}
      />
    </main>
  {:else if view === 'channel'}
    <main id="chwrap">
      {#if channelStrip}
        {@const cs = channelStrip}
        <div class="chstrip">
          <Strip
            strip={cs}
            selected={true}
            soloed={solos.includes(cs.ch)}
            muteLocked={!mutesEnabled}
            fine={fineMode}
            displayValue={displayFader(cs)}
            displayDb={displayDb(cs)}
            clip={clips.includes(cs.ch)}
            {registerMeter}
            {unregisterMeter}
            onFader={(v) => stripFader(cs, v)}
            onMute={() => toggleMute(cs)}
            onSolo={() => toggleSolo(cs.ch)}
            onSelect={() => {}}
            onValueTap={() => openValue(cs)}
            onMeterTap={() => clearClip(cs.ch)}
          />
        </div>
        {#if cs.ch >= 0}
          <div class="mgassign" aria-label="mute group assignment">
            <span class="lbl">mute groups</span>
            {#each mgroups as g, i (g.name)}
              <button
                class:on={g.members.includes(cs.ch)}
                onclick={() => mgToggle(cs.ch, i)}
                title="assign {cs.name} to {g.name}"
              >{g.name}</button>
            {/each}
          </div>
        {/if}
        <div class="chdetail">
          <Detail strip={selectedStrip} {hidden} spectrum={spectra[selected] ?? null} onParam={pushParam} onName={pushName} />
        </div>
      {:else}
        <div class="empty">no channel selected</div>
      {/if}
    </main>
  {:else if view === 'shows'}
    <main id="showswrap">
      <ShowEditor strips={strips} backend={backend} say={say} />
    </main>
  {:else}
    <main id="wrap">
      {#key layer + String(inSof)}
        <div id="strips" transition:fade={{ duration: 120 }}>
          {#if !mixerCells.length}
            <div class="empty">{layer === 'buses' && !busStrips.length ? 'no buses on this backend yet' : 'empty bank — LAYOUT › reset restores backend order'}</div>
          {:else}
            {#each mixerCells as c, i (c.kind === 'gap' ? c.gid : `ch${c.kind === 'strip' ? c.s.ch : i}${c.kind === 'strip' && c.master ? 'M' : ''}`)}
              {#if c.kind === 'gap'}
                <div class="spacer" title="blank spacer"></div>
              {:else}
                <div in:fly={{ y: 14, duration: 160, delay: Math.min(i, 8) * 25 }} class:masterpin={c.master}>
                  {#if c.master}
                    <div class="pinlabel" style={`background: ${sofColorV}`}>MASTER</div>
                  {/if}
                  <Strip
                    strip={c.s}
                    selected={c.s.ch === selected}
                    dimmed={solos.length > 0 && !solos.includes(c.s.ch)}
                    soloed={solos.includes(c.s.ch)}
                    muteLocked={!mutesEnabled}
                    fine={fineMode}
                    displayValue={displayFader(c.s)}
                    displayDb={displayDb(c.s)}
                    sofColor={inSof && !c.master ? sofColorV : null}
                    clip={clips.includes(c.s.ch)}
                    editMode={editLayout && !inSof}
                    canRemove={!(layers.find((l) => l.id === layer)?.builtin) || c.kind === 'strip'}
                    onMove={(d) => moveEntry(i, d)}
                    onGap={() => addGap(i)}
                    onRemove={() => removeEntry(i)}
                    {registerMeter}
                    {unregisterMeter}
                    onFader={(v) => stripFader(c.kind === 'strip' ? c.s : mainPseudo(), v)}
                    onMute={() => toggleMute(c.kind === 'strip' ? c.s : mainPseudo())}
                    onSolo={() => toggleSolo(c.kind === 'strip' ? c.s.ch : -1)}
                    onSelect={() => select(c.kind === 'strip' ? c.s.ch : -1)}
                    onValueTap={() => openValue(c.kind === 'strip' ? c.s : mainPseudo())}
                    onMeterTap={() => clearClip(c.kind === 'strip' ? c.s.ch : -1)}
                  />
                </div>
              {/if}
            {/each}
          {/if}
        </div>
      {/key}
    </main>
  {/if}

  {#if editing}
    <ValueEntry
      title={editing.title}
      sub={editing.sub}
      value={editing.value}
      format={editing.isSend ? (v: number): string => fmtSendDb(faderToDb(v)) : faderDb}
      onApply={applyValue}
      onCancel={() => { editing = null; }}
    />
  {/if}
</div>

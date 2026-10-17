<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { fade, fly } from 'svelte/transition';
  import { selectBackend } from './lib/backend';
  import { swipe } from './lib/swipe';
  import type { Backend, ClientsInfo, LockHolds, MainStrip, ParamValue, Unsub, Strip as StripData } from './lib/backend';
  import { eqBandsFor, ensureBandsFor, grFor, noteBandParam } from './lib/bands';
  import { levels, setLevels } from './lib/levels';
  import { paintStageMeter } from './lib/meters';
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
  import MixerPane from './components/MixerPane.svelte';
  import SplitView from './components/SplitView.svelte';
  import MeterBridge from './components/MeterBridge.svelte';
  import Detail from './components/Detail.svelte';
  import FxRack from './components/FxRack.svelte';
  import GainWizard from './components/GainWizard.svelte';
  import Onboarding from './components/Onboarding.svelte';
  import Overview from './components/Overview.svelte';
  import ShowEditor from './components/ShowEditor.svelte';
  import ValueEntry from './components/ValueEntry.svelte';
  import MusicianView from './components/MusicianView.svelte';
  import ClientsRoster from './components/ClientsRoster.svelte';
  import SurfaceView from './components/SurfaceView.svelte';

  const backend: Backend = selectBackend();
  const via = backend.kind === 'tauri' ? 'tauri ipc' : `ws ${backend.url}`;

  const MAIN: MainStrip = { ch: -1, name: 'MAIN' };

  type View = 'overview' | 'mixer' | 'surface' | 'channel' | 'rack' | 'shows' | 'wizard';
  type Cell = { kind: 'strip'; s: StripData; master?: boolean } | { kind: 'gap'; gid: string };
  interface Editing { ch: number; title: string; sub: string; value: number; isSend: boolean; bus: number | null }

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
    return v === 'overview' || v === 'mixer' || v === 'surface' || v === 'channel' || v === 'rack' || v === 'shows' || v === 'wizard' ? v : 'mixer';
  }

  /* ---------- state ---------- */
  let strips: StripData[] = $state(defaultStrips());
  let online: boolean = $state(false);
  let status: string = $state('');
  let view: View = $state(loadView());
  let layer: string = $state('inputs');
  let layerB: string = $state('buses'); // split-mode pane B layer (own bank)
  let split: boolean = $state(loadJSON<boolean>('openmix.split.v1', false));
  let layers: LayerDef[] = $state(loadLayers());
  let orders: Record<string, OrderEntry[]> = $state(loadOrders());
  let mgroups: MuteGroup[] = $state(loadJSON<MuteGroup[]>(LS.groups, defaultGroups()));
  let sends: SendMap = $state(loadSends());
  let mutesEnabled: boolean = $state(loadJSON<boolean>(LS.muteEn, true));
  let fineMode: boolean = $state(loadJSON<boolean>(LS.fine, false));
  let sofBus: number | null = $state(null); // transient: sends-on-fader destination
  let sofBusB: number | null = $state(null); // split-mode pane B SoF destination
  let musicianMode: boolean = $state(false); // performer wedge mix: only their bus
  let musicianBus: number | null = $state(null); // musician's picked destination bus
  let clips: number[] = $state([]); // latched clip channels, tap meter to clear
  let editing: Editing | null = $state(null); // tap-a-value modal
  let editLayout: boolean = $state(false);
  let tick: number = $state(0); // ~8Hz pulse for the Overview health rows
  let selected: number = $state(0);
  // Surface status bar: active show name (ShowEditor's localStorage slot).
  let showName: string = $state(loadJSON<string>('openmix.shows.active.v1', 'openmix'));
  let solos: number[] = $state([]);
  let hidden: string[] = $state([]); // param fields the backend rejected
  const locksSupported = backend.kind === 'ws';
  const clientsSupported = backend.kind === 'ws';
  let clients: ClientsInfo | null = $state(null); // relay roster (null until the first `clients` frame)
  let locks: LockHolds = $state({}); // ch -> holder client id (empty = no pins / no locks feed)
  let myLocks: number[] = $state([]); // channels this client claimed (confirmed by the holds table)
  let storeArm: boolean = $state(false);
  let driver: string = $state('sim');
  let addr: string = $state('');
  let showOnboard: boolean = $state(false);
  const ONBOARD_KEY = 'openmix.onboarded.v1';
  function closeOnboard(): void {
    showOnboard = false;
    try {
      localStorage.setItem(ONBOARD_KEY, '1');
    } catch {
      /* private mode: wizard simply reappears next load */
    }
  }
  let showBridge: boolean = $state(false);
  let menuOpen: boolean = $state(false); // topbar overflow menu (narrow screens)
  let detailOpen: boolean = $state(true); // channel detail bottom-sheet collapse
  let master: { fader: number } = $state({ fader: 0.8 }); // LR main mirror (driver-backed via onMain/setMainFader; 0.8 until first state)
  const meterMapA = new Map<number, HTMLCanvasElement>(); // ch -> canvas (imperative paint, no re-render)
  const meterMapB = new Map<number, HTMLCanvasElement>(); // split-mode pane B canvases (same ch can show twice)
  let spectra: Record<number, number[]> = $state({}); // ch -> bins (live spectrum; null until relay sends it)
  let gr: Record<number, { gate: number; comp: number }> = $state({}); // ch -> live GR dB (null until relay sends it)
  let gains: Record<number, number> = {}; // detail gain dB per ch (staging display only)

  let lastMeterAt = 0;
  let lastTick = 0;
  const registerMeter = (ch: number, cv: HTMLCanvasElement): void => {
    meterMapA.set(ch, cv);
  };
  const unregisterMeter = (ch: number): void => {
    meterMapA.delete(ch);
  };
  const registerMeterB = (ch: number, cv: HTMLCanvasElement): void => {
    meterMapB.set(ch, cv);
  };
  const unregisterMeterB = (ch: number): void => {
    meterMapB.delete(ch);
  };

  /* ---------- helpers ---------- */
  const isBusStrip = (s: StripData): boolean => isBusCh(s.ch) || /^bus/i.test(s.name);
  const inSof = $derived(sofBus !== null);
  const sofColorV = $derived(sofBus === null ? '' : busColor(sofBus));
  const sofName = $derived(sofBus === null ? '' : (strips.find((s) => s.ch === sofBus)?.name ?? `bus ${sofBus}`));
  const sofColorB = $derived(sofBusB === null ? '' : busColor(sofBusB));
  const sofNameB = $derived(sofBusB === null ? '' : (strips.find((s) => s.ch === sofBusB)?.name ?? `bus ${sofBusB}`));
  const busStrips: StripData[] = $derived(strips.filter(isBusStrip));
  // Musician mode mirrors (read-only derives; writes go through setSend/setFader).
  const musicianInputs: StripData[] = $derived(strips.filter((s) => !isBusStrip(s)));
  const musicianBusName = $derived(
    musicianBus === null ? '' : (strips.find((s) => s.ch === musicianBus)?.name ?? `bus ${musicianBus}`),
  );
  const musicianBusColor = $derived(musicianBus === null ? '' : busColor(musicianBus));
  const musicianMaster = $derived(
    musicianBus === null ? 0.8 : (strips.find((s) => s.ch === musicianBus)?.fader ?? 0.8),
  );

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

  /** Bank cells for any (layer, SoF) pair — shared by single + split panes. */
  function cellsFor(layerId: string, sof: number | null): Cell[] {
    if (sof !== null) {
      const bus = sof;
      const cells: Cell[] = strips.filter((s) => !isBusStrip(s)).map((s): Cell => ({ kind: 'strip', s }));
      const b = strips.find((s) => s.ch === bus);
      if (b) cells.push({ kind: 'strip', s: b, master: true });
      return cells;
    }
    const ord = orders[layerId] ?? defaultOrderFor(layerId);
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
  }

  let mixerCells: Cell[] = $derived.by(() => cellsFor(layer, sofBus));
  let cellsB: Cell[] = $derived.by(() => cellsFor(layerB, sofBusB));

  /** Fader position actually shown for a strip (send level in SoF). */
  function displayFaderFor(s: StripData, sof: number | null): number {
    if (s.main || s.ch < 0) return master.fader;
    if (sof !== null && !isBusStrip(s)) return dbToFader(sendDb(sends, s.ch, sof));
    return s.fader;
  }

  function displayDbFor(s: StripData, sof: number | null): string {
    if (s.main || s.ch < 0) return faderDb(master.fader);
    if (sof !== null && !isBusStrip(s)) return fmtSendDb(sendDb(sends, s.ch, sof));
    return faderDb(s.fader);
  }

  function displayFader(s: StripData): number {
    return displayFaderFor(s, sofBus);
  }

  function displayDb(s: StripData): string {
    return displayDbFor(s, sofBus);
  }

  const say = (t: string): void => {
    status = t || '';
  };
  const clamp01 = (v: number): number => Math.min(1, Math.max(0, v));
  const errMsg = (e: unknown): string => (e instanceof Error ? e.message : String(e));

  /* ---------- fader / mute / select ---------- */
  /** LR main fader: optimistic mirror, authoritative value arrives via onMain. */
  async function setMainFader(v: number): Promise<void> {
    v = clamp01(v);
    const prev = master.fader;
    master.fader = v; // optimistic for touch latency
    try {
      await backend.setMainFader(v);
    } catch (e) {
      master.fader = prev;
      say(`main: ${errMsg(e)}`);
    }
  }

  async function setFader(s: StripData, v: number): Promise<void> {
    v = clamp01(v);
    if (s.main || s.ch < 0) {
      await setMainFader(v);
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
  async function setSend(ch: number, bus: number, v: number): Promise<void> {
    const db = Math.round(faderToDb(v) * 2) / 2;
    sends[sendKey(ch, bus)] = db;
    saveSends(sends);
    try {
      await backend.setParam(ch, auxField(bus), db);
    } catch (e) {
      say(`send ch${ch + 1}→${strips.find((s) => s.ch === bus)?.name ?? bus}: ${errMsg(e)} (kept locally)`);
    }
  }

  function stripFaderFor(s: StripData, sof: number | null, v: number): Promise<void> {
    // In SoF, input strips drive sends; the pinned bus master drives its fader.
    if (sof !== null && !s.main && s.ch >= 0 && !isBusStrip(s)) return setSend(s.ch, sof, v);
    return setFader(s, v);
  }

  function stripFader(s: StripData, v: number): Promise<void> {
    return stripFaderFor(s, sofBus, v);
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

  /* ---------- strip locks (multi-client arbitration) ---------- */
  const lockEntries = $derived(
    Object.entries(locks)
      .map(([k, v]): [number, string] => [Number(k), v])
      .filter(([ch, h]) => Number.isInteger(ch) && ch >= 0 && !!h)
      .sort((a, b) => a[0] - b[0]),
  );

  async function toggleLock(ch: number): Promise<void> {
    if (!locksSupported || ch < 0) return;
    if (myLocks.includes(ch)) {
      // Release our own hold (optimistic; the locks frame confirms).
      myLocks = myLocks.filter((c) => c !== ch);
      try {
        await backend.unlockStrip(ch);
      } catch (e) {
        say(`unlock ch${ch + 1}: ${errMsg(e)}`);
      }
      return;
    }
    if (locks[ch] !== undefined) {
      // Held by someone else: ask anyway — the denial names the holder.
      try {
        await backend.lockStrip(ch);
      } catch (e) {
        say(`lock ch${ch + 1}: ${errMsg(e)}`);
      }
      return;
    }
    // Unlocked: claim optimistically; a denial rolls this back.
    myLocks = [...myLocks, ch];
    try {
      await backend.lockStrip(ch);
    } catch (e) {
      myLocks = myLocks.filter((c) => c !== ch);
      say(`lock ch${ch + 1}: ${errMsg(e)}`);
    }
  }

  function select(ch: number): void {
    selected = ch;
  }

  function selectLayer(id: string): void {
    layer = id;
  }

  function selectLayerB(id: string): void {
    layerB = id;
  }

  function layersKey(e: KeyboardEvent): void {
    const ids = layers.map((l) => l.id);
    const i = ids.indexOf(layer);
    if (e.key === 'ArrowRight' || e.key === 'ArrowLeft') {
      const d = e.key === 'ArrowRight' ? 1 : -1;
      const next = ids[(i + d + ids.length) % ids.length];
      if (next) {
        selectLayer(next);
        e.preventDefault();
      }
    } else if (e.key === 'Home' || e.key === 'End') {
      const next = e.key === 'Home' ? ids[0] : ids[ids.length - 1];
      if (next) {
        selectLayer(next);
        e.preventDefault();
      }
    }
  }

  function jumpTo(ch: number): void {
    selected = ch;
    setView('mixer');
  }

  /** Step selection through the current bank (phone arrows / swipe). */
  function stepStrip(dir: -1 | 1): void {
    const ids = mixerCells.flatMap((c): number[] => (c.kind === 'strip' ? [c.s.ch] : []));
    if (!ids.length) return;
    const at = ids.indexOf(selected);
    const i = at < 0 ? (dir > 0 ? 0 : ids.length - 1) : (at + dir + ids.length) % ids.length;
    const next = ids[i];
    if (next === undefined) return;
    selected = next;
    requestAnimationFrame(() => {
      document.querySelector(`[data-strip="${next}"]`)?.scrollIntoView({ block: 'nearest', inline: 'center', behavior: 'smooth' });
    });
  }

  /** Step selection through every strip (channel view stepper). */
  function stepChannel(dir: -1 | 1): void {
    const ids = [...strips.map((s) => s.ch), -1];
    if (!ids.length) return;
    const at = ids.indexOf(selected);
    const i = at < 0 ? 0 : (at + dir + ids.length) % ids.length;
    const next = ids[i];
    if (next !== undefined) selected = next;
  }

  function clearClip(ch: number): void {
    if (clips.includes(ch)) clips = clips.filter((c) => c !== ch);
  }

  /* ---------- sends-on-fader ---------- */
  function enterSofPane(pane: 'a' | 'b', busCh: number): void {
    if (busIdx(busCh) < 0) {
      say('that bus has no aux send tap on this backend');
      return;
    }
    if (pane === 'a') sofBus = busCh;
    else sofBusB = busCh;
    if (!solos.includes(busCh)) solos = [...solos, busCh]; // auto-cue the destination
    setView('mixer');
    say(`sends → ${strips.find((s) => s.ch === busCh)?.name ?? busCh}: faders are sends — EXIT returns to LR`);
  }

  function enterSof(busCh: number): void {
    enterSofPane('a', busCh);
  }

  function exitSofPane(pane: 'a' | 'b'): void {
    const bus = pane === 'a' ? sofBus : sofBusB;
    if (bus !== null) solos = solos.filter((c) => c !== bus);
    if (pane === 'a') sofBus = null;
    else sofBusB = null;
    say('back to LR');
  }

  function exitSof(): void {
    exitSofPane('a');
  }

  /* ---------- musician monitor mode (pick-your-bus wedge mix) ---------- */
  function toggleMusician(): void {
    musicianMode = !musicianMode;
    say(musicianMode ? 'musician mode: pick your wedge — everything else locked out' : 'back to FOH');
  }

  function exitMusician(): void {
    musicianMode = false;
    say('back to FOH');
  }

  function callFoh(): void {
    if (musicianBus === null) return;
    // Best-effort `attention` broadcast over the EXISTING param path: the
    // relay fans every param frame out to all clients, so a FOH build with
    // a toast listener flashes there; builds without one ignore the unknown
    // field and the performer's local strobe still covers the UX.
    backend.setParam(musicianBus, 'attention', 1).catch((e: unknown) => {
      say(`call FOH stayed local: ${errMsg(e)}`);
    });
  }

  /* ---------- FOH attention toasts (musician CALL FOH) ---------- */
  interface Attention {
    id: number;
    ch: number;
  }
  let attentions: Attention[] = $state([]);
  let attnSeq = 0;
  const ATTN_TTL_MS = 10_000;
  const ATTN_MAX = 3;

  function attnName(ch: number): string {
    return strips.find((s) => s.ch === ch)?.name ?? `bus ${ch}`;
  }

  function pushAttention(ch: number): void {
    // FOH surface only: the musician box already has its local strobe.
    if (musicianMode) return;
    attnSeq += 1;
    const id = attnSeq;
    attentions = [...attentions.filter((a) => a.ch !== ch), { id, ch }].slice(-ATTN_MAX);
    window.setTimeout(() => {
      attentions = attentions.filter((a) => a.id !== id);
    }, ATTN_TTL_MS);
  }

  function dismissAttention(id: number): void {
    attentions = attentions.filter((a) => a.id !== id);
  }

  function jumpToAttention(a: Attention): void {
    dismissAttention(a.id);
    enterSof(a.ch);
  }

  async function copyLrPane(pane: 'a' | 'b'): Promise<void> {
    const bus = pane === 'a' ? sofBus : sofBusB;
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
    say(`copied LR (${ins.length} sends) → ${strips.find((s) => s.ch === bus)?.name ?? bus}`);
  }

  async function copyLrToBus(): Promise<void> {
    await copyLrPane('a');
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
  function orderOf(layerId: string): OrderEntry[] {
    return orders[layerId] ?? defaultOrderFor(layerId);
  }

  function setOrder(layerId: string, next: OrderEntry[]): void {
    orders[layerId] = next;
    saveOrders();
  }

  function moveEntry(layerId: string, idx: number, dir: -1 | 1): void {
    const ord = [...orderOf(layerId)];
    const j = idx + dir;
    if (idx < 0 || idx >= ord.length || j < 0 || j >= ord.length) return;
    const [e] = ord.splice(idx, 1);
    if (e) ord.splice(j, 0, e);
    setOrder(layerId, ord);
  }

  function addGap(layerId: string, idx: number): void {
    const ord = [...orderOf(layerId)];
    ord.splice(idx + 1, 0, { t: 'gap', gid: `gap-${Date.now()}-${idx}` });
    setOrder(layerId, ord);
  }

  function removeEntry(layerId: string, idx: number): void {
    const ord = [...orderOf(layerId)];
    ord.splice(idx, 1);
    setOrder(layerId, ord);
  }

  function resetLayout(layerId: string): void {
    delete orders[layerId];
    saveOrders();
    say(`${layerId} layout reset to backend order`);
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
  function openValueFor(s: StripData, sof: number | null): void {
    if (s.main || s.ch < 0) {
      editing = { ch: -1, title: 'MAIN', sub: 'master fader', value: master.fader, isSend: false, bus: null };
      return;
    }
    if (sof !== null && !isBusStrip(s)) {
      editing = {
        ch: s.ch,
        title: `${s.name} → ${strips.find((x) => x.ch === sof)?.name ?? sof}`,
        sub: 'send level',
        value: dbToFader(sendDb(sends, s.ch, sof)),
        isSend: true,
        bus: sof,
      };
      return;
    }
    editing = { ch: s.ch, title: s.name, sub: 'fader', value: s.fader, isSend: false, bus: null };
  }

  function openValue(s: StripData): void {
    openValueFor(s, sofBus);
  }

  async function applyValue(v: number): Promise<void> {
    const e = editing;
    editing = null;
    if (!e) return;
    if (e.isSend) {
      if (e.bus !== null) await setSend(e.ch, e.bus, v);
      return;
    }
    if (e.ch < 0) {
      await setMainFader(v);
      return;
    }
    const s = strips.find((x) => x.ch === e.ch);
    if (s) await setFader(s, v);
  }

  /* ---------- detail params ---------- */
  async function pushParam(field: string, v: ParamValue): Promise<void> {
    if (field === 'gain' && typeof v === 'number') gains[selected] = v; // staging display
    if (selected >= 0) noteBandParam(selected, field, v); // mixer-thumb mirror (relay never echoes params)
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

  /** Channel-addressed param push for the Surface bank/editor knobs.
   * Same unsupported-gracefully contract as pushParam (selected channel). */
  async function pushParamFor(ch: number, field: string, v: ParamValue): Promise<void> {
    if (ch >= 0) noteBandParam(ch, field, v); // mixer-thumb mirror (relay never echoes params)
    try {
      await backend.setParam(ch, field, v);
    } catch (e) {
      if (!hidden.includes(field)) hidden = [...hidden, field];
      say(`${field}: unsupported (${errMsg(e)})`);
    }
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

  function paintMeter(ch: number, cv: HTMLCanvasElement): void {
    const s = ch === -1 ? null : strips.find((x) => x.ch === ch);
    const f = ch === -1 ? master.fader : s ? displayFader(s) : 0;
    const raw = ch === -1 ? f * 0.9 : !s ? 0 : (levels[s.ch] ?? 0);
    if (!clips.includes(ch) && raw >= 0.985) clips = [...clips, ch]; // latch
    const input = clamp01(raw / Math.max(f, 0.08)); // pre-fader estimate
    paintStageMeter(cv, s?.muted ? 0 : input, s?.muted ? 0 : raw, clips.includes(ch));
  }

  function loop(now: number): void {
    for (const [ch, cv] of meterMapA) paintMeter(ch, cv);
    for (const [ch, cv] of meterMapB) paintMeter(ch, cv);
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

  onMount(() => {
    // Surface replaces the Mixer tab on wide landscape screens (and hands
    // it back on narrow/portrait) — phone portrait + existing views untouched.
    const wideMq = window.matchMedia('(min-width: 1100px) and (orientation: landscape)');
    const applyWide = (): void => {
      if (wideMq.matches && view === 'mixer') setView('surface');
      else if (!wideMq.matches && view === 'surface') setView('mixer');
    };
    applyWide();
    wideMq.addEventListener('change', applyWide);
    if (!layers.some((l) => l.id === layer)) layer = 'inputs';
    if (!layers.some((l) => l.id === layerB)) layerB = layers.some((l) => l.id === 'buses') ? 'buses' : 'inputs';
    try {
      if (localStorage.getItem(ONBOARD_KEY) !== '1') showOnboard = true;
    } catch {
      showOnboard = true; // storage blocked: show it rather than lose first-run help
    }
    const offs: Unsub[] = [
      backend.onState((next) => {
        strips = next;
        // Seed per-channel EQ copies here (subscription context, not render).
        for (const s of next) if (s.ch >= 0) ensureBandsFor(s.ch);
        if (!strips.some((s) => s.ch === selected)) selected = strips[0]?.ch ?? 0;
      }),
      // Driver-backed LR main: server state is the source of truth.
      backend.onMain((m) => {
        if (m !== null) master.fader = m;
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
    offs.push(
      backend.onGr(({ ch, gate, comp }) => {
        const next = { ...gr };
        next[ch] = { gate, comp };
        gr = next;
      }),
    );
    if (backend.onStatus) offs.push(backend.onStatus((on) => {
      online = on;
    }));
    // Strip locks: holds table drives badges + panel; denials name the holder.
    // Missing on old servers — the table simply stays empty (null-safe).
    try {
      offs.push(backend.onLocks((holds) => {
        locks = { ...(holds ?? {}) };
        const live = new Set(Object.keys(locks).map(Number));
        const kept = myLocks.filter((c) => live.has(c));
        if (kept.length !== myLocks.length) myLocks = kept;
      }));
    } catch {
      /* transport without locks */
    }
    try {
      offs.push(backend.onDeny((d) => {
        if (d.ch !== null && Number.isFinite(d.ch)) {
          myLocks = myLocks.filter((c) => c !== d.ch);
        }
        say(d.lockedBy ? `ch${(d.ch ?? -1) + 1} locked by ${d.lockedBy}` : d.message);
      }));
    } catch {
      /* transport without locks */
    }
    // Client roster: relay-only presence feed; Tauri has no `onClients`
    // (stays hidden there). Null-safe for transports without the feed.
    try {
      const off = backend.onClients?.((info) => {
        clients = {
          count: info?.count ?? 0,
          writers: [...(info?.writers ?? [])],
          selfId: info?.selfId ?? null,
        };
      });
      if (off) offs.push(off);
    } catch {
      /* transport without a clients feed */
    }
    // Musician CALL FOH: `param` `attention` frames fanned out by the relay.
    // Null-safe for old servers / transports without a param feed.
    try {
      offs.push(
        backend.onParam((evt) => {
          if (evt.field === 'attention') pushAttention(evt.ch);
        }),
      );
    } catch {
      /* transport without param frames */
    }

    const staleTimer = window.setInterval(() => {
      // Offline if no meter frame for >1.5s while expecting one.
      if (lastMeterAt && performance.now() - lastMeterAt > 1500) online = false;
    }, 1000);

    raf = requestAnimationFrame(loop);

    // Auto-connect sim for instant use; failures surface on the badge.
    backend
      .connect('sim', '')
      .then((m) => say(m))
      .catch((e: unknown) => say(errMsg(e)));

    return () => {
      offs.forEach((off) => off());
      window.clearInterval(staleTimer);
      wideMq.removeEventListener('change', applyWide);
      cancelAnimationFrame(raf);
    };
  });

  onDestroy(() => {
    if (backend.kind === 'ws') backend.close?.();
  });
</script>

<svelte:window onkeydown={onKey} />

<div id="app" class:sof={inSof && !split} class:surface={view === 'surface' && !musicianMode} style={inSof && !split ? `--sof: ${sofColorV}` : ''}>
  <!-- Console surface carries its own status bar, connect UI, SoF + locks:
       legacy chrome hides via CSS (#app.surface …) so Surface is a full
       takeover, not a view. -->
  {#if attentions.length}
    <div id="attn" role="alert" aria-live="assertive" aria-label="musician calls">
      {#each attentions as a (a.id)}
        {@const nm = attnName(a.ch)}
        <div class="attn-toast" style={`border-color: ${busColor(a.ch)}`}>
          <button
            class="attn-jump"
            onclick={() => jumpToAttention(a)}
            title={`jump to ${nm} mix`}
            aria-label={`🎙 ${nm} calling FOH — tap to jump to that mix`}
          >
            <b>🎙 {nm} calling FOH</b>
            <small>tap to jump · {Math.round(ATTN_TTL_MS / 1000)}s</small>
          </button>
          <button
            class="attn-x"
            onclick={() => dismissAttention(a.id)}
            title="dismiss"
            aria-label={`dismiss call from ${nm}`}
          >✕</button>
        </div>
      {/each}
    </div>
  {/if}

  <header id="topbar" class:open={menuOpen}>
    <span id="logo">openmix</span>
    <button
      id="menubtn"
      aria-expanded={menuOpen}
      aria-label={menuOpen ? 'close menu' : 'open menu'}
      title="menu"
      onclick={() => { menuOpen = !menuOpen; }}
    >{menuOpen ? '✕' : '☰'}</button>
    <div class="tgroup" id="tgroup">
    <label>driver
      <select bind:value={driver} onchange={() => { menuOpen = false; }}>
        <option value="sim">sim</option>
        <option value="xair">xair</option>
        <option value="dl32s">dl32s</option>
      </select>
    </label>
    <label>addr <input id="addr" inputmode="numeric" placeholder="192.168.1.10 (blank = sim)" size="22" bind:value={addr} /></label>
    <button id="conn" class="big" onclick={() => { menuOpen = false; connect(); }}>connect</button>
    <span id="badge" class:online class:offline={!online}>{online ? 'online' : 'offline'}</span>
    <span id="via" title="transport">{via}</span>
    <button id="bridgeBtn" class:on={showBridge} aria-pressed={showBridge} title="toggle meter bridge" onclick={() => { showBridge = !showBridge; }}>bridge</button>
    <button id="musicianBtn" class:on={musicianMode} aria-pressed={musicianMode} title="musician monitor mode: pick-your-bus wedge mix, everything else locked out" onclick={() => { toggleMusician(); menuOpen = false; }}>musician</button>
    <button id="helpBtn" title="first-run wizard & guided tour" aria-label="open setup wizard and tour" onclick={() => { showOnboard = true; menuOpen = false; }}>?</button>
    </div>
  </header>

  {#if musicianMode}
    <MusicianView
      inputs={musicianInputs}
      buses={busStrips}
      busCh={musicianBus}
      busName={musicianBusName}
      busColorHex={musicianBusColor}
      sendValue={(ch) => (musicianBus === null ? 0 : dbToFader(sendDb(sends, ch, musicianBus)))}
      sendLabel={(ch) => (musicianBus === null ? '-inf' : fmtSendDb(sendDb(sends, ch, musicianBus)))}
      masterValue={musicianMaster}
      masterLabel={faderDb(musicianMaster)}
      onPickBus={(b) => { musicianBus = b; }}
      onSend={(ch, v) => { if (musicianBus !== null) void setSend(ch, musicianBus, v); }}
      onMaster={(v) => { const b = musicianBus === null ? undefined : strips.find((s) => s.ch === musicianBus); if (b) void setFader(b, v); }}
      onCallFoh={callFoh}
      onExit={exitMusician}
    />
  {/if}
  <div id="fohwrap" style:display={musicianMode ? 'none' : ''}>

  <nav id="views" aria-label="views">
    {#each [{ id: 'overview', label: 'Overview' }, { id: 'mixer', label: 'Mixer' }, { id: 'surface', label: 'Surface' }, { id: 'channel', label: 'Channel' }, { id: 'rack', label: 'Rack' }, { id: 'shows', label: 'Shows' }, { id: 'wizard', label: 'Wizard' }] as v (v.id)}
      <button class:on={view === v.id} class:nav-mixer={v.id === 'mixer'} class:nav-surface={v.id === 'surface'} aria-current={view === v.id ? 'page' : undefined} onclick={() => setView(v.id as View)}>{v.label}</button>
    {/each}
  </nav>

  <!-- QSC-style aux rail: always-visible bus selects, one tap flips to that mix. -->
  {#if view !== 'channel' && busStrips.length && !split}
    <nav id="auxrail" aria-label="mix destination">
      <button class:lr={!inSof} aria-pressed={!inSof} onclick={exitSof} title="back to LR">LR</button>
      {#each busStrips as b (b.ch)}
        <button
          class:dest={sofBus === b.ch}
          aria-pressed={sofBus === b.ch}
          style={sofBus === b.ch ? `border-color: ${busColor(b.ch)}; color: ${busColor(b.ch)}` : ''}
          onclick={() => (sofBus === b.ch ? exitSof() : enterSof(b.ch))}
          title={`sends-on-fader to ${b.name}`}
          aria-label={`sends-on-fader to ${b.name}`}
        >{b.name}</button>
      {/each}
    </nav>
  {/if}

  <!-- SoF banner: the mode can never be mistaken for LR. -->
  {#if inSof && !split}
    <div id="sofbanner" style={`--sof: ${sofColorV}`} role="alert">
      <b>SENDS → {sofName}</b>
      <span>faders are sends, not LR</span>
      <button onclick={copyLrToBus} title="start from current LR balance">copy LR</button>
      <button class="exit" onclick={exitSof} title="exit sends-on-fader (same gesture as entry)">EXIT ✕</button>
    </div>
  {/if}

  {#if view === 'mixer'}
    <div id="layers" role="tablist" aria-label="fader banks" tabindex={0} onkeydown={layersKey}>
      {#each layers as l (l.id)}
        <button
          role="tab"
          data-layer={l.id}
          class:on={layer === l.id}
          aria-selected={layer === l.id}
          tabindex={layer === l.id ? 0 : -1}
          onclick={() => { selectLayer(l.id); }}
        >
          {l.name}
        </button>
      {/each}
    </div>

    <div id="toolbar">
      <button
        id="muteEn"
        class:on={mutesEnabled}
        aria-pressed={mutesEnabled}
        title="master mute-enable: when off, all mute buttons lock out"
        onclick={() => { mutesEnabled = !mutesEnabled; saveJSON(LS.muteEn, mutesEnabled); say(mutesEnabled ? 'mutes armed' : 'mutes locked out'); }}
      >MUTE EN</button>
      <button
        class:on={fineMode}
        aria-pressed={fineMode}
        title="fine-adjust: fader drags move at 20%, smaller key steps"
        onclick={() => { fineMode = !fineMode; saveJSON(LS.fine, fineMode); }}
      >FINE</button>
      <button class:on={editLayout} aria-pressed={editLayout} title="edit bank layout: reorder, spacers, custom layers" onclick={() => { editLayout = !editLayout; }}>LAYOUT</button>
      <button
        class:on={split}
        aria-pressed={split}
        title="dual-pane: two independent mixer views side by side (FOH + monitor). Stacks below 1100px."
        onclick={() => { split = !split; saveJSON('openmix.split.v1', split); say(split ? 'dual-pane on: A and B mix independently' : 'single pane'); }}
      >SPLIT</button>
      {#if editLayout}
        <button onclick={addLayer} title="new custom layer from current strips">+layer</button>
        <button onclick={renameLayer} title="rename current layer tab">rename</button>
        <button onclick={() => resetLayout(layer)} title="restore backend order">reset</button>
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
          aria-label={g.members.length ? `toggle mute group ${g.name}, ${g.members.length} channels` : `mute group ${g.name}, empty`}
        >{g.name}{g.members.length ? `·${g.members.length}` : ''}</button>
      {/each}
    </div>
  {/if}

  {#if showBridge}
    <div transition:fade={{ duration: 150 }}>
      <MeterBridge
        {strips}
        {selected}
        {solos}
        {clips}
        {tick}
        onNavigate={jumpTo}
        onSolo={toggleSolo}
        onClearClip={clearClip}
      />
    </div>
  {/if}

  <div id="scenes">
    <span class="lbl">scenes</span>
    <div id="slots">
      {#each [1, 2, 3, 4] as i (i)}
        <div class="slot">
          <b>{i}{scenes[i] ? '●' : '○'}</b>
        <button
            title={scenes[i] ? `tap: recall scene ${i}` : `scene ${i} empty — arm store, then tap`}
            aria-label={scenes[i] ? `recall scene ${i}` : `scene ${i} empty — arm store first`}
            onclick={() => (storeArm ? storeScene(i) : recallScene(i))}
          >S{i}</button>
        </div>
      {/each}
    </div>
    <button id="store" class:armed={storeArm} aria-pressed={storeArm} onclick={() => { storeArm = !storeArm; say(storeArm ? 'store armed — tap a slot' : ''); }}>store</button>
    <span id="status" role="status">{status}</span>
  </div>

  {#if clientsSupported && clients && clients.count > 0}
    <ClientsRoster count={clients.count} writers={clients.writers} selfId={clients.selfId} />
  {/if}

  {#if locksSupported && lockEntries.length}
    <div id="locks" role="status" aria-label="strip locks">
      <span class="lbl">locks</span>
      {#each lockEntries as [ch, holder] (ch)}
        {@const mine = myLocks.includes(ch)}
        {@const nm = strips.find((s) => s.ch === ch)?.name ?? `ch${ch + 1}`}
        {#if mine}
          <button class="lk mine" onclick={() => toggleLock(ch)} title={`you hold ${nm} — tap to release`} aria-label={`release your lock on ${nm}`}>● {nm} · mine</button>
        {:else}
          <span class="lk held" title={`held by ${holder}`}>● {nm} · {holder}</span>
        {/if}
      {/each}
    </div>
  {/if}

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
    <main id="chwrap" class:sheetclosed={!detailOpen}>
      {#if channelStrip}
        {@const cs = channelStrip}
        <div id="chstep" aria-label="step channels">
          <button aria-label="previous channel" title="previous channel" onclick={() => stepChannel(-1)}>‹</button>
          <span>{cs.ch < 0 ? 'MAIN' : `${cs.name} · ch${cs.ch + 1}`}</span>
          <button aria-label="next channel" title="next channel" onclick={() => stepChannel(1)}>›</button>
          {#if sofBus !== null}
            <span class="chsof" style={`--sof: ${sofColorV}`}>SENDS → {sofName}</span>
          {/if}
        </div>
        <div
          class="chstrip"
          use:swipe={stepChannel}
        >
          <Strip
            strip={cs}
            selected={true}
            soloed={solos.includes(cs.ch)}
            muteLocked={!mutesEnabled}
            fine={fineMode}
            displayValue={displayFader(cs)}
            displayDb={displayDb(cs)}
            clip={clips.includes(cs.ch)}
            bands={cs.ch >= 0 ? eqBandsFor(cs.ch) : null}
            grDb={grFor(cs.ch, gr)}
            lockHolder={locksSupported ? (locks[cs.ch] ?? null) : null}
            lockMine={myLocks.includes(cs.ch)}
            onLockToggle={locksSupported ? () => toggleLock(cs.ch) : undefined}
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
                aria-pressed={g.members.includes(cs.ch)}
                onclick={() => mgToggle(cs.ch, i)}
                title={`assign ${cs.name} to ${g.name}`}
                aria-label={`assign ${cs.name} to mute group ${g.name}`}
              >{g.name}</button>
            {/each}
          </div>
        {/if}
        <div class="chdetail" class:closed={!detailOpen}>
          <button
            id="sheethandle"
            aria-expanded={detailOpen}
            aria-label={detailOpen ? 'collapse channel detail' : 'expand channel detail'}
            title={detailOpen ? 'collapse detail' : 'expand detail'}
            onclick={() => { detailOpen = !detailOpen; }}
          ><span aria-hidden="true"></span>{detailOpen ? 'detail ▾' : 'detail ▴'}</button>
          <div class="sheetbody">
          <Detail strip={selectedStrip} {hidden} spectrum={spectra[selected] ?? null} grGate={gr[selected]?.gate ?? null} grComp={gr[selected]?.comp ?? null} onParam={pushParam} onName={pushName} />
          </div>
        </div>
      {:else}
        <div class="empty">no channel selected</div>
      {/if}
    </main>
  {:else if view === 'rack'}
    <main id="rackwrap">
      <FxRack backend={backend} say={say} />
    </main>
  {:else if view === 'shows'}
    <main id="showswrap">
      <ShowEditor strips={strips} backend={backend} say={say} />
    </main>
  {:else if view === 'wizard'}
    <main id="wizwrap">
      <GainWizard backend={backend} strips={strips} say={say} />
    </main>
  {:else if view === 'surface'}
    <main id="surfacewrap">
      <SurfaceView
        {showName}
        {online}
        {via}
        cells={mixerCells}
        {strips}
        {busStrips}
        {selected}
        {solos}
        {clips}
        {mutesEnabled}
        fine={fineMode}
        sofBus={sofBus}
        sofColor={sofColorV}
        sofName={sofName}
        {gr}
        {locks}
        {myLocks}
        {locksSupported}
        {hidden}
        {displayFader}
        {displayDb}
        onFader={(s, v) => stripFader(s, v)}
        onMute={(s) => toggleMute(s)}
        onSolo={(ch) => toggleSolo(ch)}
        onSelect={(ch) => select(ch)}
        onLockToggle={(ch) => toggleLock(ch)}
        onValueTap={(s) => openValue(s)}
        onMeterTap={(ch) => clearClip(ch)}
        {registerMeter}
        {unregisterMeter}
        selectedStrip={selectedStrip}
        spectrum={spectra[selected] ?? null}
        grGate={gr[selected]?.gate ?? null}
        grComp={gr[selected]?.comp ?? null}
        onParam={pushParam}
        onParamFor={pushParamFor}
        onName={pushName}
        {sends}
        onSend={(ch, bus, v) => setSend(ch, bus, v)}
        master={mainPseudo()}
        masterFader={master.fader}
        onMaster={(v) => setMainFader(v)}
        {mgroups}
        onToggleGroup={(i) => toggleGroup(i)}
        {backend}
        {say}
        {driver}
        {addr}
        onDriver={(d) => { driver = d; }}
        onAddr={(a) => { addr = a; }}
        onConnect={() => { void connect(); }}
        {tick}
        onNavigate={(ch) => select(ch)}
        onRoute={(ch, bus) => { select(ch); enterSof(bus); }}
      />
    </main>
  {:else if split}
    <main id="splitwrap">
      <SplitView>
        {#snippet paneA()}
          <MixerPane
            label="A"
            {layers}
            {layer}
            cells={mixerCells}
            buses={busStrips}
            {sofBus}
            sofColor={sofColorV}
            {sofName}
            {selected}
            {solos}
            {clips}
            {mutesEnabled}
            fine={fineMode}
            {editLayout}
            {gr}
            {locks}
            {myLocks}
            {locksSupported}
            {displayFader}
            {displayDb}
            onLayer={selectLayer}
            onFader={(s, v) => stripFader(s, v)}
            onMute={(s) => toggleMute(s)}
            onSolo={(ch) => toggleSolo(ch)}
            onSelect={(ch) => select(ch)}
            onValueTap={(s) => openValue(s)}
            onMeterTap={(ch) => clearClip(ch)}
            onMove={(idx, d) => moveEntry(layer, idx, d)}
            onGap={(idx) => addGap(layer, idx)}
            onRemove={(idx) => removeEntry(layer, idx)}
            onEnterSof={(b) => enterSofPane('a', b)}
            onExitSof={() => exitSofPane('a')}
            onCopyLr={() => { void copyLrPane('a'); }}
            onLockToggle={(ch) => toggleLock(ch)}
            {registerMeter}
            {unregisterMeter}
          />
        {/snippet}
        {#snippet paneB()}
          <MixerPane
            label="B"
            {layers}
            layer={layerB}
            cells={cellsB}
            buses={busStrips}
            sofBus={sofBusB}
            sofColor={sofColorB}
            sofName={sofNameB}
            {selected}
            {solos}
            {clips}
            {mutesEnabled}
            fine={fineMode}
            {editLayout}
            {gr}
            {locks}
            {myLocks}
            {locksSupported}
            displayFader={(s) => displayFaderFor(s, sofBusB)}
            displayDb={(s) => displayDbFor(s, sofBusB)}
            onLayer={selectLayerB}
            onFader={(s, v) => stripFaderFor(s, sofBusB, v)}
            onMute={(s) => toggleMute(s)}
            onSolo={(ch) => toggleSolo(ch)}
            onSelect={(ch) => select(ch)}
            onValueTap={(s) => openValueFor(s, sofBusB)}
            onMeterTap={(ch) => clearClip(ch)}
            onMove={(idx, d) => moveEntry(layerB, idx, d)}
            onGap={(idx) => addGap(layerB, idx)}
            onRemove={(idx) => removeEntry(layerB, idx)}
            onEnterSof={(b) => enterSofPane('b', b)}
            onExitSof={() => exitSofPane('b')}
            onCopyLr={() => { void copyLrPane('b'); }}
            onLockToggle={(ch) => toggleLock(ch)}
            registerMeter={registerMeterB}
            unregisterMeter={unregisterMeterB}
          />
        {/snippet}
      </SplitView>
    </main>
  {:else}
    <main id="wrap">
      {#key layer + String(inSof)}
        <div id="phonenav" aria-label="step channels">
          <button aria-label="previous channel" title="previous channel" onclick={() => stepStrip(-1)}>‹</button>
          <span>{selected < 0 ? 'MAIN' : (strips.find((s) => s.ch === selected)?.name ?? `ch${selected + 1}`)}</span>
          <button aria-label="next channel" title="next channel" onclick={() => stepStrip(1)}>›</button>
          <button class="phdetail" title="open channel detail" aria-label="open channel detail" onclick={() => setView('channel')}>detail</button>
        </div>
        <div
          id="strips"
          transition:fade={{ duration: 120 }}
          use:swipe={stepStrip}
        >
          {#if !mixerCells.length}
            <div class="empty">{layer === 'buses' && !busStrips.length ? 'no buses on this backend yet' : 'empty bank — LAYOUT › reset restores backend order'}</div>
          {:else}
            {#each mixerCells as c, i (c.kind === 'gap' ? c.gid : `ch${c.kind === 'strip' ? c.s.ch : i}${c.kind === 'strip' && c.master ? 'M' : ''}`)}
              {#if c.kind === 'gap'}
                <div class="spacer" title="blank spacer"></div>
              {:else}
                <div data-strip={c.kind === 'strip' ? c.s.ch : -99} class="stripwrap" in:fly={{ y: 14, duration: 160, delay: Math.min(i, 8) * 25 }} class:masterpin={c.master}>
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
                    bands={c.s.ch >= 0 ? eqBandsFor(c.s.ch) : null}
                    grDb={grFor(c.s.ch, gr)}
                    editMode={editLayout && !inSof}
                    canRemove={!(layers.find((l) => l.id === layer)?.builtin) || c.kind === 'strip'}
                    onMove={(d) => moveEntry(layer, i, d)}
                    onGap={() => addGap(layer, i)}
                    onRemove={() => removeEntry(layer, i)}
                    lockHolder={locksSupported ? (locks[c.s.ch] ?? null) : null}
                    lockMine={myLocks.includes(c.s.ch)}
                    onLockToggle={locksSupported ? () => toggleLock(c.kind === 'strip' ? c.s.ch : -1) : undefined}
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

  </div><!-- /#fohwrap: FOH surface (hidden in musician mode) -->

  {#if showOnboard}
    <Onboarding backend={backend} {online} onDone={closeOnboard} onGoto={(v) => setView(v)} />
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

<style>
  /* Topbar overflow menu: hamburger hidden on wide screens. */
  #menubtn { display: none; }
  .tgroup { display: contents; }
  /* Phone channel stepper: hidden on wide screens (see app.css). */
  #phonenav { display: none; }
  /* Channel-view stepper row. */
  #chstep {
    display: none; width: 100%; gap: 8px; align-items: center;
    padding: 4px 2px 8px;
  }
  #chstep span { flex: 1; text-align: center; font-weight: 800; font-size: clamp(15px, 4vw, 18px); }
  #chstep button { font-size: 22px; font-weight: 800; padding: 0 18px; }
  #chstep .chsof {
    flex: 0 0 auto; font-size: 11px; font-weight: 800; color: var(--sof, var(--yel));
    border: 1px solid var(--sof, var(--yel)); border-radius: 6px; padding: 2px 6px;
  }
  /* Bottom-sheet handle for channel detail. */
  #sheethandle {
    display: none; width: 100%; align-items: center; justify-content: center; gap: 8px;
    background: none; border: 1px solid var(--line); border-radius: 10px;
    color: var(--dim); font-weight: 800; font-size: 13px; text-transform: uppercase;
    letter-spacing: 0.6px; margin-bottom: 6px;
  }
  #sheethandle span[aria-hidden="true"] {
    display: inline-block; width: 44px; height: 4px; border-radius: 2px; background: var(--dim);
  }
  /* Narrow topbar: hamburger + dropdown (scoped here so it beats .tgroup). */
  @media (max-width: 820px) {
    #menubtn { display: inline-flex; align-items: center; justify-content: center; font-size: 20px; }
    .tgroup { display: none; }
    #topbar.open .tgroup {
      display: flex; flex-basis: 100%; flex-wrap: wrap; gap: 8px;
      background: var(--panel); border: 1px solid var(--line); border-radius: 12px; padding: 10px;
    }
    #topbar.open .tgroup label { flex: 1 1 100%; }
    #topbar.open .tgroup input#addr { flex: 1; width: 100%; }
  }
  /* Tablet portrait and down: channel stepper + collapsible detail sheet. */
  @media (max-width: 900px) {
    #chstep { display: flex; }
    #sheethandle { display: flex; }
    .chdetail.closed > .sheetbody { display: none; }
  }
  /* Phone: channel-at-a-time stepper row (static — only the topbar sticks). */
  @media (max-width: 600px) {
    #phonenav {
      display: flex; gap: 8px; align-items: center;
      padding: 8px max(10px, env(safe-area-inset-right)) 0 max(10px, env(safe-area-inset-left));
    }
    #phonenav span { flex: 1; text-align: center; font-weight: 800; font-size: clamp(15px, 4.5vw, 19px); }
    #phonenav button { font-size: 24px; font-weight: 800; padding: 0 20px; min-width: 56px; min-height: 48px; }
    #phonenav button.phdetail { font-size: 13px; padding: 0 14px; min-width: var(--touch); text-transform: uppercase; letter-spacing: 0.5px; }
  }
  /* Surface replaces the Mixer tab on wide landscape; narrower screens
     keep the current views (the matchMedia redirect in onMount agrees). */
  @media (min-width: 1100px) and (orientation: landscape) {
    #views button.nav-mixer { display: none; }
  }
  @media (max-width: 1099px), (orientation: portrait) {
    #views button.nav-surface { display: none; }
  }
  /* CALL FOH attention toasts: status/badge shape, yellow ring like the
     musician CALL FOH button. Stacked newest-last, max 3, 10s TTL. */
  #attn {
    display: flex; flex-direction: column; gap: 6px;
    padding: 8px 10px 0;
  }
  .attn-toast {
    display: flex; gap: 10px; align-items: center; width: 100%;
    background: #4a2b00; border: 2px solid var(--yel); color: var(--txt);
    border-radius: 12px; padding: 6px 6px 6px 12px; min-height: var(--touch);
    animation: attnblink 0.7s steps(2) infinite;
  }
  .attn-jump {
    flex: 1; display: flex; gap: 10px; align-items: center; min-width: 0;
    background: none; border: 0; padding: 0; color: inherit;
    font-size: 16px; cursor: pointer; text-align: left; min-height: var(--touch);
  }
  .attn-jump b { font-weight: 800; }
  .attn-jump small { color: var(--yel); margin-left: auto; white-space: nowrap; }
  .attn-x {
    min-width: var(--touch); min-height: var(--touch);
    border: 1px solid var(--line); border-radius: 8px; color: var(--dim);
  }
  @keyframes attnblink { 50% { box-shadow: 0 0 14px var(--yel); } }
</style>

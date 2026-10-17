<script lang="ts">
  // Shows editor: ordered library of named, recallable mixer states (gig-simple).
  //
  // Shape mirrors crates/core/src/show.rs (Show { name, scenes }, Scene
  // { name, strips: [{ch,name,fader,muted}], main_fader }) so the server
  // library (crates/serve `shows` event / Tauri scene commands) deserializes
  // straight through lib/backend.ts.
  //
  // Sync strategy: the server library is the source of truth while connected
  // (server wins on connect and on every `shows` push; local-only shows are
  // kept). Scene capture/recall/delete push through the backend scene ops;
  // show create/rename/delete have no server op and stay local. localStorage
  // is the offline cache: it seeds the UI and receives every update, so a
  // dropped transport degrades to this-device recall via setFader/setMute.
  import { normShow, normShows } from '../lib/backend';
  import type { Backend, ShowLib, ShowScene, Strip } from '../lib/backend';

  /** Ordered library of scenes (cf. core Show). */
  type Show = ShowLib;
  /** Named, recallable scene (cf. core Scene). */
  type Scene = ShowScene;

  interface Props {
    strips: Strip[];
    backend: Backend;
    say?: (t: string) => void;
  }

  let { strips, backend, say }: Props = $props();

  const LS_SHOWS = 'openmix.shows.v1';
  const LS_ACTIVE = 'openmix.shows.active.v1';
  const LEGACY_SLOTS = 'openmix.scenes.v1'; // App.svelte snapshot slots S1..S4
  const FADE_DEFAULT = 400; // cf. core DEFAULT_FADE_MS
  const FADE_STEPS = 20; // cf. core DEFAULT_FADE_STEPS
  const EPS = 0.0015; // fader drift tolerance for the dirty `*`

  const tell = (t: string): void => say?.(t);
  const errMsg = (e: unknown): string => (e instanceof Error ? e.message : String(e));
  const sleep = (ms: number): Promise<void> => new Promise((r) => setTimeout(r, ms));
  const clamp01 = (v: number): number => (Number.isFinite(v) ? Math.min(1, Math.max(0, v)) : 0.75);

  function loadShows(): Show[] {
    try {
      const raw: unknown = JSON.parse(localStorage.getItem(LS_SHOWS) || 'null');
      const libs = normShows(raw);
      return libs.length ? libs : importLegacy();
    } catch {
      return importLegacy();
    }
  }

  /** One-time import of App.svelte S1..S4 slots so nothing gig-tested is lost. */
  function importLegacy(): Show[] {
    try {
      const raw: unknown = JSON.parse(localStorage.getItem(LEGACY_SLOTS) || 'null');
      if (raw === null || typeof raw !== 'object' || Array.isArray(raw)) return [];
      const rec = raw as Record<string, unknown>;
      const scenes: unknown[] = [];
      for (const k of Object.keys(rec).sort()) {
        const arr = rec[k];
        if (!Array.isArray(arr)) continue;
        scenes.push({ name: `S${k}`, strips: arr, main_fader: null });
      }
      const show = normShow({ name: 'Imported', scenes });
      return show && show.scenes.length ? [show] : [];
    } catch {
      return [];
    }
  }

  function persist(): void {
    try {
      localStorage.setItem(LS_SHOWS, JSON.stringify(shows));
      localStorage.setItem(LS_ACTIVE, JSON.stringify(activeName));
    } catch {
      /* storage blocked: library simply doesn't persist */
    }
  }

  function initialActive(loaded: Show[]): string {
    try {
      const a: unknown = JSON.parse(localStorage.getItem(LS_ACTIVE) || 'null');
      if (typeof a === 'string' && loaded.some((s) => s.name === a)) return a;
    } catch {
      /* fall through */
    }
    return loaded[0]?.name ?? '';
  }

  const initial: Show[] = loadShows();
  let shows: Show[] = $state(initial);
  let activeName: string = $state(initialActive(initial));

  let newShowName: string = $state('');
  let newSceneName: string = $state('');
  let fadeMs: number = $state(FADE_DEFAULT);
  let recalling: string | null = $state(null); // scene name currently fading
  let online: boolean = $state(false); // transport up (server ops usable)

  /**
   * Server wins on conflicts; local-only shows (created offline) are kept.
   * Remote shells left empty (all scenes deleted) are dropped — capture
   * always recreates a shell, so an empty shell only means "deleted".
   */
  function applyServer(remote: ShowLib[]): void {
    const merged: Show[] = remote
      .filter((s) => s.scenes.length > 0)
      .map((s) => ({ ...s, scenes: [...s.scenes] }));
    for (const local of shows) {
      if (!merged.some((s) => s.name === local.name)) merged.push(local);
    }
    shows = merged;
    if (!activeName || !shows.some((s) => s.name === activeName)) {
      activeName = shows[0]?.name ?? '';
    }
    persist();
  }

  /** Pull the server library (server wins); stay local when unreachable. */
  async function syncFromServer(): Promise<void> {
    try {
      applyServer(await backend.listShows());
    } catch {
      /* offline or transport without ops: keep the offline cache */
    }
  }

  $effect(() => {
    const offs = [
      backend.onShows((remote) => applyServer(remote)),
      backend.onStatus((on) => {
        const was = online;
        online = on;
        if (on && !was) void syncFromServer();
      }),
    ];
    return () => {
      offs.forEach((off) => off());
    };
  });

  const active: Show | null = $derived(shows.find((s) => s.name === activeName) ?? null);
  const liveByCh = $derived(new Map(strips.map((s) => [s.ch, s])));

  /** Live state diverged from the scene (fader or mute differs) -> `*`. */
  function isDirty(scene: Scene): boolean {
    for (const s of scene.strips) {
      const live = liveByCh.get(s.ch);
      if (!live) continue;
      if (live.muted !== s.muted) return true;
      if (Math.abs(live.fader - s.fader) > EPS) return true;
    }
    return false;
  }

  /** Scope summary: channel count + muted count. */
  function scope(scene: Scene): string {
    const m = scene.strips.filter((s) => s.muted).length;
    return `${scene.strips.length}ch${m ? ` · ${m}M` : ''}`;
  }

  function selectShow(name: string): void {
    activeName = name;
    persist();
  }

  function createShow(): void {
    const name = newShowName.trim().slice(0, 32);
    if (!name) {
      tell('show needs a name');
      return;
    }
    if (shows.some((s) => s.name === name)) {
      tell(`show "${name}" already exists`);
      return;
    }
    shows = [...shows, { name, scenes: [] }];
    activeName = name;
    newShowName = '';
    persist();
    tell(`show "${name}" created`);
  }

  function renameShow(): void {
    const show = active;
    if (!show) return;
    const name = prompt('Show name?', show.name);
    if (!name) return;
    const clean = name.trim().slice(0, 32);
    if (!clean || clean === show.name) return;
    if (shows.some((s) => s.name === clean)) {
      tell(`show "${clean}" already exists`);
      return;
    }
    show.name = clean;
    activeName = clean;
    shows = [...shows];
    persist();
    tell(`show renamed to "${clean}"`);
  }

  function deleteShow(): void {
    const show = active;
    if (!show) return;
    const condemned = show.scenes.map((s) => s.name);
    shows = shows.filter((s) => s.name !== show.name);
    activeName = shows[0]?.name ?? '';
    persist();
    // Best effort: show shells have no server op, but dropping their scenes
    // keeps the server library consistent (empty shells are pruned on merge).
    if (online && condemned.length) {
      void (async () => {
        for (const n of condemned) {
          try {
            await backend.deleteScene(show.name, n);
          } catch {
            /* stays server-side until the next successful delete */
          }
        }
      })();
    }
    tell(`show "${show.name}" deleted`);
  }

  /** Capture live state as a scene (upsert, cf. core Show::add_scene). */
  async function captureScene(): Promise<void> {
    const show = active;
    if (!show) {
      tell('create a show first');
      return;
    }
    const name = newSceneName.trim().slice(0, 32);
    if (!name) {
      tell('scene needs a name');
      return;
    }
    // Optimistic local capture: works offline, confirmed by the server push.
    const scene: Scene = {
      name,
      strips: strips.map((s) => ({ ch: s.ch, name: s.name, fader: s.fader, muted: s.muted })),
      main_fader: null,
    };
    const idx = show.scenes.findIndex((s) => s.name === name);
    if (idx >= 0) show.scenes[idx] = scene;
    else show.scenes.push(scene);
    shows = [...shows];
    newSceneName = '';
    persist();
    if (online) {
      try {
        await backend.captureScene(show.name, name);
      } catch (e) {
        tell(`scene "${name}" cached on this device (${errMsg(e)})`);
        return;
      }
    }
    tell(`scene "${name}" captured (${scene.strips.length}ch)`);
  }

  async function deleteScene(name: string): Promise<void> {
    const show = active;
    if (!show) return;
    show.scenes = show.scenes.filter((s) => s.name !== name);
    shows = [...shows];
    persist();
    if (online) {
      try {
        await backend.deleteScene(show.name, name);
      } catch (e) {
        tell(`scene "${name}" deleted locally (${errMsg(e)})`);
        return;
      }
    }
    tell(`scene "${name}" deleted`);
  }

  /** This-device fade (offline fallback; order mirrors core Scene::recall_to). */
  async function fadeLocal(scene: Scene, ms: number): Promise<void> {
    for (const s of scene.strips) {
      if (!s.muted && liveByCh.get(s.ch)?.muted) await backend.setMute(s.ch, false);
    }
    if (ms === 0) {
      for (const s of scene.strips) await backend.setFader(s.ch, s.fader);
    } else {
      const starts = scene.strips.map((s) => liveByCh.get(s.ch)?.fader ?? s.fader);
      const stepMs = ms / FADE_STEPS;
      for (let step = 1; step <= FADE_STEPS; step++) {
        const t = step / FADE_STEPS;
        for (let i = 0; i < scene.strips.length; i++) {
          const s = scene.strips[i];
          if (!s) continue;
          await backend.setFader(s.ch, clamp01((starts[i] ?? s.fader) + (s.fader - (starts[i] ?? s.fader)) * t));
        }
        if (step < FADE_STEPS && stepMs > 0) await sleep(stepMs);
      }
      for (const s of scene.strips) await backend.setFader(s.ch, s.fader);
    }
    for (const s of scene.strips) {
      if (s.muted && !liveByCh.get(s.ch)?.muted) await backend.setMute(s.ch, true);
    }
  }

  /** Recall via the server op when connected, else a this-device fade. */
  async function recallScene(scene: Scene): Promise<void> {
    if (recalling) return;
    const show = active;
    if (!show) return;
    recalling = scene.name;
    const ms = Math.max(0, Math.round(fadeMs) || 0);
    tell(`recalling "${scene.name}"…`);
    try {
      if (online) {
        try {
          await backend.recallScene(show.name, scene.name, ms);
          tell(`recalled "${scene.name}" (fade ${ms}ms)`);
          return;
        } catch {
          /* server lacks the scene (or the op): fall back to this device */
        }
      }
      await fadeLocal(scene, ms);
      tell(`recalled "${scene.name}" (fade ${ms}ms)`);
    } catch (e) {
      tell(`recall "${scene.name}": ${errMsg(e)}`);
    } finally {
      recalling = null;
    }
  }
</script>

<div id="showedit">
  {#if !shows.length}
    <div class="empty">no shows yet — name one below to start your setlist</div>
  {:else}
    <nav class="showtabs" aria-label="shows">
      {#each shows as s (s.name)}
        <button class:on={s.name === activeName} aria-current={s.name === activeName ? 'true' : undefined} onclick={() => selectShow(s.name)}>{s.name}</button>
      {/each}
    </nav>
    {#if active}
      <div class="showbar">
        <b>{active.name}</b>
        <span class="scope">{active.scenes.length} scene{active.scenes.length === 1 ? '' : 's'}</span>
        <button onclick={renameShow} title="rename this show">rename</button>
        <button onclick={deleteShow} title="delete this show and its scenes">delete</button>
      </div>
      {#if !active.scenes.length}
        <div class="empty">no scenes — capture one from the live mix below</div>
      {:else}
        <div class="scenelist" role="list" aria-label="scenes">
          {#each active.scenes as scene (scene.name)}
            {@const dirty = isDirty(scene)}
            <div class="scenerow" role="listitem">
              <button
                class="recall"
                disabled={recalling !== null}
                onclick={() => recallScene(scene)}
                title={dirty ? 'recall (mix has moved on — *)' : 'recall'}
                aria-label={dirty ? `recall scene ${scene.name} (mix differs)` : `recall scene ${scene.name}`}
              >{recalling === scene.name ? '…' : '▶'}</button>
              <span class="sname" title={dirty ? 'mix differs from this scene' : 'matches live mix'}>
                {scene.name}{dirty ? '*' : ''}
              </span>
              <span class="scope" title="channels in this scene">{scope(scene)}</span>
              <button
                class="del"
                disabled={recalling !== null}
                onclick={() => deleteScene(scene.name)}
                title={`delete scene ${scene.name}`}
                aria-label={`delete scene ${scene.name}`}
              >✕</button>
            </div>
          {/each}
        </div>
      {/if}
    {/if}
  {/if}

  <div class="editrow">
    <input
      type="text"
      placeholder="new show name"
      bind:value={newShowName}
      onkeydown={(e) => { if (e.key === 'Enter') createShow(); }}
      aria-label="new show name"
    />
    <button onclick={createShow}>+ show</button>
  </div>

  <div class="editrow">
    <input
      type="text"
      placeholder="scene name (e.g. verse)"
      bind:value={newSceneName}
      disabled={!active}
      onkeydown={(e) => { if (e.key === 'Enter') captureScene(); }}
      aria-label="new scene name"
    />
    <button onclick={captureScene} disabled={!active} title="snapshot the live mix into this show">capture</button>
    <label class="fade">fade
      <input
        type="number"
        min="0"
        max="10000"
        step="50"
        bind:value={fadeMs}
        aria-label="recall fade milliseconds"
      />ms
    </label>
  </div>
  <div class="hint">`*` = mix differs from the scene. Capture snapshots live faders + mutes; recall fades faders then snaps mutes. {online ? 'Library syncs with the server (server wins).' : 'Offline — library is on this device and merges on connect.'}</div>
</div>

<style>
  #showedit { padding: 12px 10px; max-width: 700px; margin: 0 auto; display: flex; flex-direction: column; gap: 10px; }
  .showtabs { display: flex; gap: 8px; overflow-x: auto; }
  .showtabs button.on { background: #12395a; border-color: var(--acc); font-weight: 800; }
  .showbar { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; background: var(--panel); border: 1px solid var(--line); border-radius: 10px; padding: 8px 10px; }
  .showbar b { font-size: 17px; }
  .scope { color: var(--dim); font-size: 13px; font-variant-numeric: tabular-nums; }
  .scenelist { display: flex; flex-direction: column; gap: 6px; }
  .scenerow { display: flex; gap: 8px; align-items: center; background: var(--panel); border: 1px solid var(--line); border-radius: 10px; padding: 6px 8px; }
  .recall { border-color: var(--grn); color: var(--grn); font-weight: 800; }
  .sname { flex: 1; font-weight: 700; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .del { color: var(--dim); }
  .editrow { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
  .editrow input[type='text'] { flex: 1; min-width: 140px; }
  .fade { display: flex; gap: 6px; align-items: center; color: var(--dim); font-size: 13px; }
  .fade input[type='number'] { background: #0a0c0e; color: var(--txt); border: 1px solid var(--line); border-radius: 8px; min-height: var(--touch); padding: 0 8px; font-size: 15px; width: 90px; }
  .hint { color: var(--dim); font-size: 13px; }
</style>

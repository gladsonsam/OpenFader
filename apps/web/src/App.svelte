<script>
  import { onMount, onDestroy } from 'svelte';
  import { fade, fly } from 'svelte/transition';
  import { selectBackend } from './lib/backend.js';
  import { levels, setLevels } from './lib/levels.js';
  import { paintMeter, paintBridge } from './lib/meters.js';
  import Strip from './components/Strip.svelte';
  import Detail from './components/Detail.svelte';

  const backend = selectBackend();
  const via = backend.kind === 'tauri' ? 'tauri ipc' : `ws ${backend.url}`;

  const defaultStrips = () =>
    Array.from({ length: 8 }, (_, i) => ({
      ch: i,
      name: `CH${String(i + 1).padStart(2, '0')}`,
      fader: 0.75,
      muted: false,
    }));

  /* ---------- state ---------- */
  let strips = $state(defaultStrips());
  let online = $state(false);
  let status = $state('');
  let layer = $state('inputs');
  let selected = $state(0);
  let solos = $state([]);
  let hidden = $state([]); // param fields the backend rejected
  let storeArm = $state(false);
  let driver = $state('sim');
  let addr = $state('');
  let showBridge = $state(false);
  let master = $state({ fader: 0.8 }); // local monitor-dim until backend exposes main
  let lastMeterAt = 0;

  let bridge = $state(null);
  const meterMap = new Map(); // ch -> canvas (imperative paint, no re-render)
  const registerMeter = (ch, cv) => meterMap.set(ch, cv);
  const unregisterMeter = (ch) => { meterMap.delete(ch); };

  /* ---------- helpers ---------- */
  const isBus = (s) => /^bus/i.test(s.name);

  let visible = $derived.by(() => {
    if (layer === 'mains')
      return [{ ch: -1, name: 'MAIN', fader: master.fader, muted: false, main: true }];
    const buses = strips.filter(isBus);
    const inputs = strips.filter((s) => !isBus(s));
    if (layer === 'buses') return buses.length ? buses : inputs.slice(-8);
    if (buses.length) return inputs;
    return strips;
  });

  let selectedStrip = $derived(
    [...strips, { ch: -1, name: 'MAIN' }].find((x) => x.ch === selected) ?? null,
  );

  const say = (t) => { status = t || ''; };
  const clamp01 = (v) => Math.min(1, Math.max(0, v));

  /* ---------- fader / mute / select ---------- */
  async function setFader(s, v) {
    v = clamp01(v);
    if (s.main) { master.fader = v; return; }
    const cur = strips.find((x) => x.ch === s.ch);
    if (cur) cur.fader = v; // optimistic for touch latency
    try { await backend.setFader(s.ch, v); }
    catch (e) { say(`fader ch${s.ch + 1}: ${e.message || e}`); }
  }

  async function toggleMute(s) {
    if (s.main) return;
    const next = !s.muted;
    const cur = strips.find((x) => x.ch === s.ch);
    if (cur) cur.muted = next;
    try { await backend.setMute(s.ch, next); }
    catch (e) {
      if (cur) cur.muted = !next;
      say(`mute ch${s.ch + 1}: ${e.message || e}`);
    }
  }

  function toggleSolo(ch) {
    solos = solos.includes(ch) ? solos.filter((c) => c !== ch) : [...solos, ch];
  }

  function select(ch) { selected = ch; }

  /* ---------- detail params ---------- */
  async function pushParam(field, v) {
    try { await backend.setParam(selected, field, v); }
    catch (e) {
      if (!hidden.includes(field)) hidden = [...hidden, field];
      say(`${field}: unsupported (${e.message || e})`);
    }
  }

  function pushName(v) { pushParam('name', v); }

  /* ---------- scenes (local snapshot slots) ---------- */
  const SCENE_KEY = 'openmix.scenes.v1';
  function loadScenes() {
    try { return JSON.parse(localStorage.getItem(SCENE_KEY)) || {}; }
    catch { return {}; }
  }
  let scenes = $state(loadScenes());

  function storeScene(i) {
    const all = loadScenes();
    all[i] = strips.map((s) => ({ ...s }));
    localStorage.setItem(SCENE_KEY, JSON.stringify(all));
    scenes = all;
    storeArm = false;
    say(`scene ${i} stored (${strips.length} strips)`);
  }

  async function recallScene(i) {
    const snap = loadScenes()[i];
    if (!snap) { say(`scene ${i} empty — tap store, then S${i}`); return; }
    say(`recalling scene ${i}…`);
    for (const s of snap) {
      const cur = strips.find((x) => x.ch === s.ch);
      if (!cur) continue;
      try {
        if (cur.fader !== s.fader) await backend.setFader(s.ch, s.fader);
        if (cur.muted !== s.muted) await backend.setMute(s.ch, s.muted);
        cur.fader = s.fader;
        cur.muted = s.muted;
      } catch (e) { say(`scene ${i}: ch${s.ch + 1} ${e.message || e}`); break; }
    }
    say(`scene ${i} recalled`);
  }

  /* ---------- connect ---------- */
  async function connect() {
    say(`connecting ${driver} ${addr.trim() || '(sim)'}…`);
    try {
      const msg = await backend.connect(driver, addr.trim());
      say(msg);
    } catch (e) { say(`connect failed: ${e.message || e}`); }
  }

  /* ---------- meter loop ---------- */
  let raf = 0;

  function sizeBridge() {
    if (!bridge) return;
    bridge.width = bridge.clientWidth * devicePixelRatio;
    bridge.height = 64 * devicePixelRatio;
  }

  function loop() {
    for (const [ch, cv] of meterMap) {
      const s = ch === -1 ? null : strips.find((x) => x.ch === ch);
      const v = !s ? 0 : s.main ? s.fader * 0.9 : (levels[s.ch] ?? 0);
      paintMeter(cv, s?.muted ? 0 : v);
    }
    if (showBridge && bridge) {
      const frame = strips.map((s, i) => (s.muted ? 0 : (levels[i] ?? 0)));
      paintBridge(bridge, frame);
    }
    raf = requestAnimationFrame(loop);
  }

  function onKey(e) {
    if (/INPUT|SELECT|TEXTAREA/.test(document.activeElement?.tagName || '')) return;
    const s = strips.find((x) => x.ch === selected);
    if (!s) return;
    const step = e.shiftKey ? 0.05 : 0.01;
    if (e.key === 'ArrowUp' || e.key === 'ArrowRight') { setFader(s, s.fader + step); e.preventDefault(); }
    else if (e.key === 'ArrowDown' || e.key === 'ArrowLeft') { setFader(s, s.fader - step); e.preventDefault(); }
    else if (e.key === 'm' || e.key === 'M') toggleMute(s);
  }

  function onResize() { if (showBridge) sizeBridge(); }

  onMount(() => {
    const offs = [
      backend.onState((next) => {
        strips = next;
        if (!strips.some((s) => s.ch === selected)) selected = strips[0]?.ch ?? 0;
      }),
      backend.onMeters((lv) => { setLevels(lv); lastMeterAt = performance.now(); }),
    ];
    if (backend.onStatus) offs.push(backend.onStatus((on) => { online = on; }));

    const staleTimer = setInterval(() => {
      // Offline if no meter frame for >1.5s while expecting one.
      if (lastMeterAt && performance.now() - lastMeterAt > 1500) online = false;
    }, 1000);

    if (showBridge) sizeBridge();
    raf = requestAnimationFrame(loop);
    window.addEventListener('resize', onResize);

    // Auto-connect sim for instant use; failures surface on the badge.
    backend.connect('sim', '').then((m) => say(m)).catch((e) => say(e.message || e));

    return () => {
      offs.forEach((off) => off());
      clearInterval(staleTimer);
      cancelAnimationFrame(raf);
      window.removeEventListener('resize', onResize);
    };
  });

  onDestroy(() => { if (backend.kind === 'ws') backend.close?.(); });

  $effect(() => {
    // Size the bridge canvas once it becomes visible.
    if (showBridge && bridge) sizeBridge();
  });
</script>

<svelte:window onkeydown={onKey} />

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

<nav id="layers" role="tablist">
  {#each ['inputs', 'buses', 'mains'] as l (l)}
    <button role="tab" data-layer={l} class:on={layer === l} onclick={() => { layer = l; }}>
      {l[0].toUpperCase() + l.slice(1)}
    </button>
  {/each}
</nav>

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

<main id="wrap">
  {#key layer}
    <div id="strips" transition:fade={{ duration: 120 }}>
      {#if !visible.length}
        <div class="empty">{layer === 'buses' ? 'no buses on this backend yet' : 'no strips'}</div>
      {:else}
        {#each visible as s, i (s.ch)}
          <div in:fly={{ y: 14, duration: 160, delay: Math.min(i, 8) * 25 }}>
            <Strip
              strip={s}
              selected={s.ch === selected}
              dimmed={solos.length > 0 && !solos.includes(s.ch)}
              soloed={solos.includes(s.ch)}
              {registerMeter}
              {unregisterMeter}
              onFader={(v) => setFader(s, v)}
              onMute={() => toggleMute(s)}
              onSolo={() => toggleSolo(s.ch)}
              onSelect={() => select(s.ch)}
            />
          </div>
        {/each}
      {/if}
    </div>
  {/key}
  <aside id="detail" aria-label="channel detail">
    {#key selected}
      <Detail strip={selectedStrip} {hidden} onParam={pushParam} onName={pushName} />
    {/key}
  </aside>
</main>

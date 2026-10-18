<!-- W2a debug screen: live engine inspector (verification only, not design).
     URL: ?ui=4&screen=world-debug&server=127.0.0.1:8080[&ch=3] -->
<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { EngineWorld } from '../../world/engineWorld/engineWorld.ts';
  import { WsTransport, wsUrlFromLocation } from '../../world/transport.ts';
  import { rangeMismatches } from '../../world/mapping/params.ts';
  import type { WorldBank, WorldStrip } from '../../world/types.ts';

  const query = new URLSearchParams(location.search);
  const serverParam = query.get('server') || '127.0.0.1:8080';
  const url =
    /^wss?:\/\//i.test(serverParam) || /^https?:\/\//i.test(serverParam)
      ? serverParam
      : wsUrlFromLocation();
  const focusCh = Math.max(0, parseInt(query.get('ch') || '0', 10) || 0);

  const transport = new WsTransport(url);
  const world = new EngineWorld(transport);
  (window as unknown as { __world?: EngineWorld }).__world = world;

  interface Snap {
    tick: number;
    version: number;
    online: boolean;
    driver: string;
    selfId: string | null;
    clients: string;
    writers: string;
    link: string;
    info: string;
    deny: string;
    lawId: string;
    banks: WorldBank[];
    strips: WorldStrip[];
    stripKnown: string[];
    detail: string[];
    meterSize: number;
    gr0: string;
    tapNames: string;
    capsReady: boolean;
    unsupported: string;
    firstValues: string;
    counts: string;
  }

  function snapshot(tick: number): Snap {
    const eq = world.channelEq(focusCh);
    const dyn = world.channelDyn(focusCh);
    const sends = world.channelSends(focusCh);
    const inp = world.channelInput(focusCh);
    const detail: string[] = [
      `eq.on=${eq.on.value}[${eq.on.status}] hpf=${eq.hpf.on.value}[${eq.hpf.on.status}] f=${eq.hpf.f.value}[${eq.hpf.f.status}] slope=${eq.hpf.slope.value}[${eq.hpf.slope.status}]`,
      ...eq.bands.map(
        (b, i) =>
          `eq${i + 1}: ${b.type} g=${b.g.value}[${b.g.status}] f=${b.f.value}[${b.f.status}] q=${b.q.value}[${b.q.status}] raw=${b.rawType}`,
      ),
      ...(eq.band5 ? [`eq5: ${eq.band5.type} g=${eq.band5.g.value}[${eq.band5.g.status}]`] : []),
      `gate.on=${dyn.gate.on.value}[${dyn.gate.on.status}] thr=${dyn.gate.thr.value}[${dyn.gate.thr.status}] range=${dyn.gate.range.value} att=${dyn.gate.att.value} hold=${dyn.gate.hold.value} rel=${dyn.gate.rel.value}`,
      `comp.on=${dyn.comp.on.value}[${dyn.comp.on.status}] thr=${dyn.comp.thr.value}[${dyn.comp.thr.status}] ratio=${dyn.comp.ratio.value} att=${dyn.comp.att.value} rel=${dyn.comp.rel.value} knee=${dyn.comp.knee.value} makeup=${dyn.comp.makeup.value}`,
      `comp_hold(param-level)=${world.param(focusCh, 'comp_hold')}[${world.status(focusCh, 'comp_hold')}]`,
      `input: gain=${inp.gain.value}[${inp.gain.status}] trim=${inp.trim.value} phantom=${inp.phantom.value} invert=${inp.invert.value} delayOn=${inp.delayOn.value} delayMs=${inp.delayMs.value}`,
      'sends: ' +
        sends
          .map((s, i) =>
            s.value.value !== null || s.on.value !== null
              ? `s${i + 1}=${s.value.value?.toFixed(2)}[${s.value.status}]${s.pre.value === null ? '' : s.pre.value ? 'pre' : 'post'}`
              : '',
          )
          .filter(Boolean)
          .join(' '),
    ];
    const ids = ['fader', 'mute', 'name', 'gain', 'pan', 'eq1_gain', 'gate_thr', 'comp_thr', 'send1_level'];
    const strips: WorldStrip[] = [];
    const stripKnown: string[] = [];
    for (let ch = 0; ch < Math.min(12, world.stripCount); ch++) {
      strips.push(world.strip(ch));
      let known = 0;
      for (const id of ids) {
        const st = world.status(ch, id);
        if (st === 'known' || st === 'pending') known++;
      }
      stripKnown.push(`${known}/${ids.length}`);
    }
    let firstValues = '—';
    if (world.connectedAt !== null) {
      const times: number[] = [];
      for (let ch = 0; ch < Math.min(16, world.stripCount); ch++) {
        for (const id of ids) {
          const t = world.firstValueAt(ch, id);
          if (t !== null) times.push(t);
        }
      }
      firstValues = times.length
        ? `${times.length} values, last ${((Math.max(...times) - world.connectedAt) / 1000).toFixed(2)} s after connect`
        : 'no values yet';
    }
    return {
      tick,
      version: world.version,
      online: world.online,
      driver: world.driver,
      selfId: world.selfId,
      clients: String(world.clients.count),
      writers: world.clients.writers.join(','),
      link: world.link ? `${world.link.state} ${world.link.name} ${world.link.ip} rtt=${world.link.latencyMs.toFixed(1)}ms` : '—',
      info: world.info ? `${world.info.model} fw=${world.info.firmware}` : '—',
      deny: world.lastDeny ? `${world.lastDeny.message} (ch=${world.lastDeny.ch})` : 'none',
      lawId: world.lawId,
      banks: world.banks,
      strips,
      stripKnown,
      detail,
      meterSize: world.meters.size,
      gr0: `gate=${world.meters.grGate[0]?.toFixed(2)} comp=${world.meters.grComp[0]?.toFixed(2)}`,
      tapNames: Object.keys(world.meters.taps).join(',') || 'none negotiated',
      capsReady: world.capsReady,
      unsupported: world.unsupported.join(', '),
      firstValues,
      counts: `shows=${world.shows.length} scenes=${world.numberedScenes.length} fxSlots=${world.fxSlots.length} patches=${world.patches.length} routing=${world.routingSupported} devices=${world.devices.length} offline=${world.offlineCount}`,
    };
  }

  let snap = $state<Snap | null>(null);
  let tick = $state(0);
  let spark: HTMLCanvasElement | null = $state(null);
  let fps = $state(0);
  let frames = 0;
  let fpsT0 = performance.now();
  const mismatches = rangeMismatches();

  function refresh(): void {
    tick++;
    snap = snapshot(tick);
  }
  const unsub = world.subscribe(refresh);

  function db(f: number): string {
    const d = world.faderToDb(f);
    return Number.isFinite(d) ? d.toFixed(1) : '-inf';
  }

  onMount(() => {
    world.connect('sim', '');
    world.setVisible(Array.from({ length: 16 }, (_, i) => i));
    world.setOpenChannel(focusCh);
    refresh();
    const iv = window.setInterval(() => {
      world.sweep(Date.now());
      drawSpark();
      refresh();
    }, 250);
    const raf = () => {
      frames++;
      const now = performance.now();
      if (now - fpsT0 >= 1000) {
        fps = Math.round((frames * 1000) / (now - fpsT0));
        frames = 0;
        fpsT0 = now;
      }
      rafId = requestAnimationFrame(raf);
    };
    let rafId = requestAnimationFrame(raf);
    return () => {
      window.clearInterval(iv);
      cancelAnimationFrame(rafId);
    };
  });

  onDestroy(() => {
    unsub();
    world.destroy();
    transport.close();
  });

  function drawSpark(): void {
    if (!spark) return;
    const ctx = spark.getContext('2d');
    if (!ctx) return;
    const m = world.meters;
    const n = Math.min(m.size, 77);
    ctx.clearRect(0, 0, spark.width, spark.height);
    ctx.fillStyle = '#111';
    ctx.fillRect(0, 0, spark.width, spark.height);
    const w = spark.width / Math.max(1, n);
    for (let i = 0; i < n; i++) {
      const v = m.level[i] ?? 0;
      const h = v * spark.height;
      ctx.fillStyle = v > 0.966 ? '#f00' : '#5cdb5c';
      ctx.fillRect(i * w, spark.height - h, Math.max(1, w - 1), h);
      const p = m.peak[i] ?? 0;
      ctx.fillStyle = '#fff';
      ctx.fillRect(i * w, spark.height - p * spark.height, Math.max(1, w - 1), 1);
    }
  }
</script>

<main style="font:12px/1.45 monospace;color:#e5e7eb;background:#0b0e13;padding:16px;min-height:100vh">
  <h1 style="font-size:16px;margin:0 0 8px">world-debug <span style="color:#6b7280">tick={tick} fps={fps} v={snap?.version}</span></h1>

  {#if snap}
    <section style="margin-bottom:12px">
      <h2 style="font-size:13px;color:#7cc4ff">connection</h2>
      <div>online={String(snap.online)} driver={snap.driver} self={snap.selfId} url={transport.url}</div>
      <div>clients={snap.clients} writers=[{snap.writers}]</div>
      <div>link={snap.link}</div>
      <div>info={snap.info}</div>
      <div>deny={snap.deny}</div>
      <div>
        <button onclick={() => world.setFader(0, Math.min(1, world.strip(0).fader + 0.05))}>ch0 fader +</button>
        <button onclick={() => world.setMute(0, !world.strip(0).muted)}>ch0 mute toggle</button>
        <button onclick={() => world.setParam(0, 'eq1_gain', ((world.param(0, 'eq1_gain') as number) ?? 0) + 1)}>ch0 eq1_gain +1</button>
        <button onclick={() => world.setParam(0, 'comp_thr', -20)}>ch0 comp_thr -20</button>
        <button onclick={() => world.writeSendLevel(0, 1, 0.5)}>ch0 send1 = 0.5 (-10 dB)</button>
        <button onclick={() => world.scan()}>scan</button>
        <button onclick={() => world.requestInfo()}>info</button>
      </div>
    </section>

    <section style="margin-bottom:12px">
      <h2 style="font-size:13px;color:#7cc4ff">banks ({snap.banks.length})</h2>
      {#each snap.banks as b}
        <div><span style="display:inline-block;width:10px;height:10px;background:{b.colour}"></span> {b.key} “{b.label}” [{b.families.join('+')}] strips {b.strips.length} ({b.strips[0]}..{b.strips[b.strips.length - 1]})</div>
      {/each}
    </section>

    <section style="margin-bottom:12px">
      <h2 style="font-size:13px;color:#7cc4ff">strips 0..11 (fader dB via {snap.lawId} law)</h2>
      {#each snap.strips as s, ch}
        <div>
          <span style="display:inline-block;width:10px;height:10px;background:{s.colour}"></span>
          ch{s.ch} {s.id} “{s.name}” src={s.source} f={s.fader.toFixed(3)} ({db(s.fader)} dB){s.muted ? ' MUTED' : ''} pan={s.pan === null ? '—' : s.pan.toFixed(2)}{s.locked ? ` LOCKED by ${s.lockedBy}` : ''} known={snap.stripKnown[ch]}
        </div>
      {/each}
    </section>

    <section style="margin-bottom:12px">
      <h2 style="font-size:13px;color:#7cc4ff">strip {focusCh} detail</h2>
      {#each snap.detail as line}
        <div>{line}</div>
      {/each}
    </section>

    <section style="margin-bottom:12px">
      <h2 style="font-size:13px;color:#7cc4ff">meters (canvas sparkline, {snap.meterSize} slots)</h2>
      <canvas bind:this={spark} width={640} height={64} style="border:1px solid #333"></canvas>
      <div>gr ch0: {snap.gr0} | taps: {snap.tapNames}</div>
    </section>

    <section style="margin-bottom:12px">
      <h2 style="font-size:13px;color:#7cc4ff">caps {snap.capsReady ? `ready (${snap.unsupported ? 1 : 0} unsupported)` : '(waiting)'}</h2>
      <div>unsupported: [{snap.unsupported}]</div>
      <div>first-values: {snap.firstValues}</div>
      <div>{snap.counts}</div>
    </section>

    <section style="margin-bottom:12px">
      <h2 style="font-size:13px;color:#7cc4ff">range mismatches (engine vs design knobs)</h2>
      {#each mismatches as m}
        <div>{m.engineId}: engine [{m.engine?.join(', ')}] vs design [{m.design?.join(', ')}] — {m.note}</div>
      {/each}
    </section>
  {/if}
</main>

<!-- W5b `eq-graph` module: the big EQ response graph. Presentational: the
     EqGraphVm in via layout `bind`, node gestures out (`select` with the
     band, `node` with band/freq/gain in Hz/dB, `q` with band/Q). SVG geometry
     verbatim from design-template.html (viewBox 0 0 1000 300, preserveAspectRatio
     none, grid strokes, dashed band fill, yellow summed curve + fill to the
     0 dB line). Imperative redraw is free: paths are recomputed only when
     the eq VM changes (no per-frame work).
     Gestures (touch/iPad-first, mouse/desktop preserved):
       - one finger on a node: drag maps the client position to freq/gain
         via eqPointFromPos (HPF freq only, clamped at 1000 Hz via
         eqNodeApply). First press also selects the band. Locked desks:
         selection still applies, value edits are dropped.
       - two fingers anywhere on the graph: PINCH adjusts Q (bandwidth) of
         the band selected when the pinch started (spread = narrower = Q
         up; HPF has no Q so pinch is a no-op there). The pinch latches
         until every finger lifts, so adding/removing a finger can never
         jump freq/gain. The graph container is touch-action:none (nodes
         already are): with pan-y the browser could claim a vertical move
         for page scroll and pointercancel the first contact before — or
         during — a two-finger pinch, which no viewport/pinch-zoom flag can
         prevent. Cost: page scroll can no longer start from the graph
         background (surrounding cards/rails still scroll); scroll intent is
         unambiguous once a second finger lands, so reliability wins.
     Keyboard (addition to the design's pointer behaviour): arrows nudge
     freq (semitone, quarter in fine) / gain (0.5 dB, 0.1 in fine);
     PageUp/PageDown nudge the focused node's Q; Home/End jump to range
     ends. Focus ring is the design's 2px #7cc4ff outline.
     Accessible Q fallback beyond the graph: every band card keeps its Freq
     / Gain / Q knobs (arrow-key sliders + double-tap reset), and the
     selected-band summary line reads Q live during a pinch.
     Audio RTA overlays render only verified driver-supplied audio_spectrum
     frames. Simulator envelope activity is separately labelled, never treated
     as audio frequencies. Source availability is explicit on the graph. -->
<script lang="ts">
  import { fade } from 'svelte/transition';
  import { REDUCED_MOTION } from '../../../primitives/feel.ts';
  import { eqNodeApply, eqPointFromPos, type EqGraphVm } from './eqModel.ts';
  import type { ModuleContext } from '../../../modules/types.ts';
  import { bridgeStrip } from '../../../state/engineBridge.ts';
  import { live } from '../../../state/live.svelte.ts';
  import { ui } from '../../../state/uiStore.svelte.ts';
  import {
    RTA_TICK_MS,
    RTA_TICK_MS_CALM,
    rtaAreaPath,
    audioRtaAreaPath,
    rtaHasSignal,
    rtaSmooth,
  } from './rtaOverlay.ts';

  let {
    props = {},
    ctx: _ctx,
    nodeId: _nodeId,
    onEvent,
  }: {
    props: { graph?: EqGraphVm | null; locked?: boolean; fine?: boolean; minH?: number; selBand?: number; selQ?: number };
    ctx: ModuleContext;
    nodeId: string;
    onEvent?: (name: string, payload?: unknown) => void;
  } = $props();

  const graph = $derived(props.graph);
  const locked = $derived(props.locked ?? false);
  const fine = $derived(props.fine ?? false);
  // Layout phone variants pass minH: 220 (the desktop value); the prop
  // exists so the layout owns the number without forking this component.
  const minH = $derived(props.minH ?? 220);

  let box: HTMLDivElement | null = null;

  // Selected band + its live Q, bound from the channel scope (store.selBand
  // / eqSelQ in the EQ layouts). Pinch snapshots both at gesture start so
  // store-echo lag can never shift the baseline mid-gesture.
  const selBand = $derived(props.selBand ?? 1);
  const selQ = $derived(props.selQ ?? 0);

  // ---- Touch gesture state (Fader.svelte pattern) ----
  // Active contacts on the graph, keyed by pointerId — EVERY contact is
  // tracked (including 3rd+ fingers and presses during the post-pinch
  // latch) so the all-fingers-lifted reset can never fire early.
  // Single-finger node drag keeps the design mapping (absolute position ->
  // freq/gain, HPF freq only). A second finger anywhere on the graph starts
  // a two-contact hold that edits Q (bandwidth) of the band selected when
  // the pair formed: spread = narrower (Q up), squeeze = wider (Q down).
  // HPF (band 0) has no Q, so an HPF-selected pair is a silent no-op hold.
  // The pair is identified by its two pointerIds (not map order), so a 3rd+
  // finger lifting can never terminate the active pinch.
  // The gesture is driven by window capture-phase listeners (installed on
  // first contact, removed when the last lifts), so it survives WKWebView
  // target retargeting; lostpointercapture is never consulted. Touch
  // pointers are implicitly captured by the platform and must NOT take an
  // explicit setPointerCapture (WKWebView drops it within a frame);
  // mouse/pen still take the grab. Once a pair has engaged it latches
  // until EVERY finger lifts (dead flag): a 2->1 transition must never
  // jump freq/gain under the leftover finger.
  let pts = new Map<number, { x: number; y: number }>();
  let single: { pid: number; band: number } | null = null;
  let pinch: { a: number; b: number; band: number; d0: number; q0: number; lastQ: number } | null = null;
  let pair = false;
  let dead = false;

  /** Q clamp/rounding, mirroring eqParamFromDrag's q branch (0.3..16, 2dp). */
  function clampQ(q: number): number {
    return Math.round(Math.min(16, Math.max(0.3, q)) * 100) / 100;
  }

  function spread(a: number, b: number): number {
    const pa = pts.get(a), pb = pts.get(b);
    if (!pa || !pb) return 0;
    return Math.hypot(pa.x - pb.x, pa.y - pb.y);
  }

  function releaseGrab(pid: number): void {
    for (const n of [box]) {
      try {
        if (n?.hasPointerCapture?.(pid)) n.releasePointerCapture(pid);
      } catch {
        /* noop */
      }
    }
  }

  function listenWindow(): void {
    if (typeof window === 'undefined') return;
    window.addEventListener('pointermove', onMove, true);
    window.addEventListener('pointerup', onUp, true);
    window.addEventListener('pointercancel', onUp, true);
  }

  function unlistenWindow(): void {
    if (typeof window === 'undefined') return;
    window.removeEventListener('pointermove', onMove, true);
    window.removeEventListener('pointerup', onUp, true);
    window.removeEventListener('pointercancel', onUp, true);
  }

  $effect(() => {
    return () => {
      unlistenWindow();
    };
  });

  function applySingle(band: number, cx: number, cy: number): void {
    if (locked || !box || !graph) return;
    const r = box.getBoundingClientRect();
    if (r.width <= 0 || r.height <= 0) return;
    const p = eqPointFromPos((cx - r.left) / r.width, (cy - r.top) / r.height);
    const a = eqNodeApply(band, p.f, p.g);
    const cur = graph.nodes[band];
    if (band === 0) onEvent?.('node', { band, f: a.f, g: cur ? cur.gain : -3 });
    else onEvent?.('node', { band, f: a.f, g: a.g });
  }

  function emitQ(q: number): void {
    if (!pinch || locked) return;
    const c = clampQ(q);
    if (c === pinch.lastQ) return;
    pinch.lastQ = c;
    onEvent?.('q', { band: pinch.band, q: c });
  }

  function down(e: PointerEvent, band: number | null): void {
    // Every contact is tracked (even during the latch) so the reset on
    // all-fingers-lifted fires exactly once. A latched (post-pair) contact
    // ignores new presses for gesture purposes until all lift.
    pts.set(e.pointerId, { x: e.clientX, y: e.clientY });
    if (dead) return;
    if (pts.size === 1) {
      // First-finger node press selects (selection applies even when locked;
      // value edits below are lock-gated). Mid-gesture presses never reselect.
      if (band !== null) {
        onEvent?.('select', { band });
        // NOTE: no value is applied here: a tap without a move selects
        // only (design nodeDown). Mapping starts on the first move.
        single = { pid: e.pointerId, band };
        if (e.pointerType !== 'touch') {
          try {
            (box ?? (e.currentTarget as HTMLElement)).setPointerCapture(e.pointerId);
          } catch {
            /* noop */
          }
        }
      }
      listenWindow();
    } else if (pts.size === 2 && !pinch && !pair) {
      // Second finger: freeze single-finger edits, start the pair on the
      // band selected NOW (not the dragged one — selection may already have
      // moved). HPF has no Q: the pair holds silently and emits nothing.
      single = null;
      pair = true;
      const ids = [...pts.keys()];
      const b = selBand;
      if (b > 0 && ids.length === 2 && ids[0] !== undefined && ids[1] !== undefined) {
        const d0 = Math.hypot(
          (pts.get(ids[1])?.x ?? 0) - (pts.get(ids[0])?.x ?? 0),
          (pts.get(ids[1])?.y ?? 0) - (pts.get(ids[0])?.y ?? 0),
        );
        const q0 = clampQ(selQ > 0 ? selQ : 1);
        // Fingers landing nearly together carry no direction yet: arm with
        // d0 = 0 and baseline on first spread >= 24px (lastQ seeds q0).
        pinch = { a: ids[0], b: ids[1], band: b, d0: d0 >= 24 ? d0 : 0, q0, lastQ: q0 };
      }
      releaseGrab(e.pointerId);
    }
    // 3rd+ fingers: tracked only (first two own the gesture).
  }

  function onMove(e: PointerEvent): void {
    if (!pts.has(e.pointerId)) return;
    pts.set(e.pointerId, { x: e.clientX, y: e.clientY });
    if (pinch) {
      // Only the pair's own contacts drive Q; a 3rd finger never perturbs it.
      if (e.pointerId !== pinch.a && e.pointerId !== pinch.b) return;
      if (locked) return;
      const d = spread(pinch.a, pinch.b);
      if (pinch.d0 <= 0) {
        if (d >= 24) {
          pinch.d0 = d;
          pinch.q0 = pinch.lastQ;
        }
        return;
      }
      // Spread (d/d0 > 1) narrows the band: Q up. Fine mode halves the
      // ratio-domain delta.
      emitQ(pinch.q0 * Math.pow(d / pinch.d0, fine ? 0.5 : 1));
      return;
    }
    if (dead || pair) return;
    if (single && e.pointerId === single.pid) applySingle(single.band, e.clientX, e.clientY);
  }

  function onUp(e: PointerEvent): void {
    if (!pts.has(e.pointerId)) return;
    pts.delete(e.pointerId);
    releaseGrab(e.pointerId);
    if (pinch && (e.pointerId === pinch.a || e.pointerId === pinch.b)) {
      pinch = null;
      pair = false;
      single = null;
      // Latch: leftover fingers are ignored until all lift (no 2->1 jump).
      if (pts.size > 0) dead = true;
    } else if (pair && pts.size < 2) {
      // HPF-selected pair (no pinch object): same latch, no value to jump.
      pair = false;
      single = null;
      if (pts.size > 0) dead = true;
    } else if (single && e.pointerId === single.pid) {
      single = null;
    }
    // A 3rd+ finger lifting while the pair is intact falls through here
    // without touching the gesture.
    if (pts.size === 0) {
      single = null;
      pinch = null;
      pair = false;
      dead = false;
      unlistenWindow();
    }
  }

  // ---- RTA overlay state (see header comment) ----
  let rtaPeaks: Float32Array | null = $state(null);
  let rtaPath = $state('');
  let rtaLabel = $state('RTA unavailable');
  let rtaStrip: string | null = null;

  // The EQ view owns its interest, released on tab/channel changes and unmount.
  // One engine client may own the desk selector at a time. Source takeover by
  // another desk client is respected; we don't continually reassert selection.
  const spectrumTarget = $derived.by(() => {
    void live.version;
    const world = live.engine;
    const ch = live.phase === 'online' && live.deskConnected && world?.capsReady && !ui.frozen
      ? bridgeStrip(world, ui.sel.id) : null;
    return world?.driver === 'xair' && ch !== null ? ch : null;
  });
  $effect(() => {
    const ch = spectrumTarget;
    const world = live.engine;
    if (world?.driver !== 'xair' || ch === null) return;
    world.setSpectrumTarget?.(ch);
    return () => world.setSpectrumTarget?.(null);
  });

  $effect(() => {
    const tick = (): void => {
      const world = live.engine;
      const ch =
        live.phase === 'online' && world !== null && world.capsReady && !ui.frozen
          ? bridgeStrip(world, ui.sel.id)
          : null;
      const connected = world?.driver === 'sim' || live.deskConnected;
      const audio = connected ? world?.meters.audioSpectrum?.() : null;
      const key = world && ch !== null ? `${world.driver}/${ch}/${audio?.epoch ?? ''}` : null;
      if (rtaStrip !== key) { rtaStrip = key; rtaPeaks = null; }
      rtaLabel = ch === null || !connected ? 'RTA unavailable · offline'
        : 'RTA unavailable · waiting for desk source';
      if (world !== null && ch !== null && connected && audio) {
        if (audio.ch === ch) {
          rtaPeaks = null; // The desk applies decay; retain its measured dB scale.
          rtaPath = audioRtaAreaPath(audio.binsDb, audio.bandEdgesHz);
          rtaLabel = `Audio RTA · ${audio.source} · ${audio.position} · −90…0 dB`;
          return;
        }
        rtaLabel = `RTA unavailable for this strip · desk source: ${audio.source} (${audio.position})`;
      }
      let bins: Float32Array | null = null;
      if (world?.driver === 'sim' && ch !== null && connected) {
        bins = world.meters.spectrum(ch);
        rtaLabel = 'Sim envelope activity · not audio RTA';
      }
      if (!bins || !rtaHasSignal(bins)) {
        rtaPeaks = null;
        rtaPath = '';
        return;
      }
      rtaPeaks = rtaSmooth(rtaPeaks, bins);
      rtaPath = rtaAreaPath(rtaPeaks);
    };
    tick();
    const iv = setInterval(tick, REDUCED_MOTION ? RTA_TICK_MS_CALM : RTA_TICK_MS);
    return () => clearInterval(iv);
  });

  function key(e: KeyboardEvent, band: number): void {
    const node = graph?.nodes[band];
    if (!node) return;
    const step = fine ? 0.25 : 1;
    let f = node.freq;
    let g = node.gain;
    if (e.key === 'ArrowLeft') f = f * Math.pow(2, -step / 12);
    else if (e.key === 'ArrowRight') f = f * Math.pow(2, step / 12);
    else if (band !== 0 && (e.key === 'ArrowUp' || e.key === 'ArrowDown')) {
      g = Math.round((g + (e.key === 'ArrowUp' ? 0.5 : -0.5) * step) * 10) / 10;
    } else if (band !== 0 && (e.key === 'PageUp' || e.key === 'PageDown')) {
      // Q keyboard fallback (pinch is touch-only): anchored on this node's
      // own Q so it is exact even when the node is not the selected band.
      if (locked) return;
      e.preventDefault();
      onEvent?.('select', { band });
      const q0 = node.q > 0 ? node.q : 1;
      const q = clampQ(q0 * Math.pow(2, (e.key === 'PageUp' ? 0.5 : -0.5) * step));
      onEvent?.('q', { band, q });
      return;
    } else if (e.key === 'Home' || e.key === 'End') {
      f = e.key === 'Home' ? 20 : band === 0 ? 1000 : 20000;
      if (band !== 0) g = e.key === 'Home' ? -15 : 15;
    } else return;
    e.preventDefault();
    onEvent?.('select', { band });
    if (locked) return;
    const a = eqNodeApply(band, f, g);
    onEvent?.('node', { band, f: a.f, g: band === 0 ? node.gain : a.g });
  }
</script>

<div
  bind:this={box}
  data-graph="1"
  data-signal-plot
  in:fade={{ duration: REDUCED_MOTION ? 0 : 120 }}
  role="group"
  aria-label="EQ graph. Drag a band node to set frequency and gain. Two-finger pinch adjusts the selected band Q."
  onpointerdown={(e) => down(e, null)}
  style="box-sizing:content-box;flex:1 1 280px;min-height:{minH}px;position:relative;border-radius:6px;border:1px solid var(--u4-line-panel);background:var(--u4-well-bg);overflow:hidden;touch-action:none"
>
  {#if graph}
    <svg
      viewBox="0 0 1000 300"
      preserveAspectRatio="none"
      style="position:absolute;inset:0;width:100%;height:100%;display:block"
      aria-hidden="true"
    >
      {#each graph.gridV as gv (gv.x + gv.stroke)}
        <line x1={gv.x} y1="0" x2={gv.x} y2="300" stroke={gv.stroke} stroke-width="1" vector-effect="non-scaling-stroke"
        ></line>
      {/each}
      {#each graph.gridH as gh (gh.y + gh.stroke)}
        <line x1="0" y1={gh.y} x2="1000" y2={gh.y} stroke={gh.stroke} stroke-width="1" vector-effect="non-scaling-stroke"
        ></line>
      {/each}
      {#if rtaPath}
        <path
          d={rtaPath}
          fill="rgba(62,208,224,0.12)"
          stroke="rgba(62,208,224,0.45)"
          stroke-width="1.2"
          vector-effect="non-scaling-stroke"
          pointer-events="none"
          aria-hidden="true"
        ></path>
      {/if}
      {#each graph.bandGhosts as ghost (ghost.band)}
        <path
          d={ghost.d}
          fill={ghost.fill}
          fill-opacity="0.55"
          stroke={ghost.stroke}
          stroke-width="1.2"
          stroke-dasharray="4 3"
          vector-effect="non-scaling-stroke"
          opacity={ghost.on ? 1 : 0.8}
          pointer-events="none"
        >
          <title>{ghost.name} {ghost.on ? 'On' : 'Off'}</title>
        </path>
      {/each}
      <path
        d={graph.bandFill}
        fill={graph.bandFillColor}
        stroke={graph.bandStroke}
        stroke-width="1.2"
        stroke-dasharray="4 3"
        vector-effect="non-scaling-stroke"
      ></path>
      <path d={graph.fillPath} fill={graph.curveFill} stroke="none"></path>
      <path
        d={graph.curvePath}
        fill="none"
        stroke={graph.curveStroke}
        stroke-width="2.5"
        vector-effect="non-scaling-stroke"
      ></path>
    </svg>
    <span style="position:absolute;right:10px;bottom:24px;max-width:85%;font-size:11px;color:var(--u4-text-8);pointer-events:none;text-align:right">{rtaLabel}</span>
    {#each graph.freqLabels as l (l.label)}
      <span
        style="position:absolute;bottom:4px;left:{l.x}%;transform:translateX(-50%);font-size:12px;color:var(--u4-text-10)"
        >{l.label}</span
      >
    {/each}
    {#each graph.gainLabels as l (l.label)}
      <span
        style="position:absolute;left:6px;top:{l.y}%;transform:translateY(-50%);font-size:12px;color:var(--u4-text-10)"
        >{l.label}</span
      >
    {/each}
    {#each graph.nodes as n (n.band)}
      <!-- svelte-ignore a11y_role_supports_aria_props_implicit -->
      <button
        class="eq-node"
        aria-label={n.aria}
        aria-valuetext={n.valueText}
        title={n.aria}
        onpointerdown={(e) => {
          e.stopPropagation();
          down(e, n.band);
        }}
        onkeydown={(e) => key(e, n.band)}
        style="position:absolute;left:{n.x}%;top:{n.y}%;width:{n.size}px;height:{n.size}px;transform:translate(-50%, -50%);border-radius:50%;border:2px solid {n.ring};background:{n.color};color:#0b0f14;font-size:13px;font-weight:600;opacity:{n.op};padding:0;touch-action:none;cursor:grab;box-shadow:0 2px 8px rgba(0,0,0,.6)"
        >{n.label}</button
      >
    {/each}
  {/if}
</div>

<style>
  button.eq-node:focus-visible {
    outline: 2px solid #7cc4ff;
    outline-offset: 1px;
  }
</style>

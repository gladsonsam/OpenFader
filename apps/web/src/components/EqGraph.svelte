<script lang="ts">
  // 4-band parametric EQ canvas: draggable balls (gain y, freq x log, Q via
  // wheel/pinch), summed curve rendering, RTA overlay.
  //
  // RTA source: `spectrum` bins prop (App subscribes to backend.onSpectrum and
  // passes spectra[selected] down). When null, falls back to a levels-derived
  // interpolation exactly like Spectrum.svelte — the panel still moves until
  // the relay sends real `{type:'spectrum'}` frames.
  //
  // Writes go through the existing `param` command as eqN_gain / eqN_freq /
  // eqN_q via the `onParam` prop — no protocol change. Band state lives in the
  // parent (Detail owns `bands`); this component mutates it in place and pushes
  // debounced/on-release so drags don't spam the relay.
  import { onMount, onDestroy } from 'svelte';
  import { levels } from '../lib/levels';
  import {
    GSHOW,
    bandDb,
    bandPoint,
    clampFreq,
    clampGain,
    clampQ,
    curveDb,
    fallbackBins,
    fmtFreq,
    freqToT,
    resampleBins,
    tToFreq,
    type EqBand,
  } from '../lib/eqCurve';
  import type { ParamValue } from '../lib/backend';

  interface Props {
    /** 0-based channel index (RTA fallback level + pushParam routing). */
    ch: number;
    /** Live band state, owned by the parent (mutated in place). */
    bands: EqBand[];
    /** Live spectrum bins, or null until the relay sends them. */
    spectrum?: number[] | null;
    /** Backend-rejected fields stay hidden (unsupported-gracefully). */
    hidden?: string[];
    onParam: (field: string, v: ParamValue) => void;
  }

  let { ch, bands, spectrum = null, hidden = [], onParam }: Props = $props();

  const COLORS = ['#f472b6', '#38bdf8', '#a3e635', '#facc15'];
  const NCOL = 96;
  const GRID_F = [50, 100, 200, 500, 1000, 2000, 5000, 10000, 20000];

  let canvas: HTMLCanvasElement | null = $state(null);
  let sel = $state(1);
  let raf = 0;
  let wheelTimer = 0;

  const visIdx = $derived([0, 1, 2, 3].filter((i) => !hidden.includes(`eq${i + 1}_gain`)));
  const selSafe = $derived(visIdx.includes(sel) ? sel : (visIdx[0] ?? 0));

  function push(n: number): void {
    const b = bands[n];
    if (!b) return;
    onParam(`eq${n + 1}_gain`, Math.round(b.gain * 2) / 2);
    onParam(`eq${n + 1}_freq`, Math.round(b.freq));
    onParam(`eq${n + 1}_q`, Math.round(b.q * 10) / 10);
  }

  function resetBand(n: number): void {
    const b = bands[n];
    if (!b) return;
    const defs = [100, 2000, 3000, 12000];
    b.gain = 0;
    b.freq = defs[n] ?? 1000;
    b.q = 1;
    push(n);
  }

  /* ---------- paint ---------- */

  function paint(): void {
    const cv = canvas;
    if (cv) {
      const dpr = window.devicePixelRatio || 1;
      const w = cv.clientWidth;
      const h = cv.clientHeight;
      if (w > 0 && h > 0) {
        if (cv.width !== Math.round(w * dpr) || cv.height !== Math.round(h * dpr)) {
          cv.width = Math.round(w * dpr);
          cv.height = Math.round(h * dpr);
        }
        const ctx = cv.getContext('2d');
        if (ctx) {
          ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
          draw(ctx, w, h);
        }
      }
    }
    raf = requestAnimationFrame(paint);
  }

  function yOf(db: number, h: number): number {
    return (1 - (db + GSHOW) / (2 * GSHOW)) * h;
  }

  function draw(ctx: CanvasRenderingContext2D, w: number, h: number): void {
    ctx.clearRect(0, 0, w, h);
    ctx.fillStyle = '#0b0f14';
    ctx.fillRect(0, 0, w, h);

    // RTA behind everything.
    const data =
      spectrum && spectrum.length
        ? resampleBins(spectrum, NCOL)
        : fallbackBins(ch >= 0 ? (levels[ch] ?? 0) : 0, NCOL);
    const cw = w / NCOL;
    for (let i = 0; i < NCOL; i++) {
      const v = data[i] ?? 0;
      const bh = Math.max(1, v * h);
      ctx.fillStyle = v < 0.55 ? 'rgba(74,222,128,0.32)' : v < 0.8 ? 'rgba(250,204,21,0.36)' : 'rgba(248,113,113,0.40)';
      ctx.fillRect(i * cw + 0.5, h - bh, cw - 1, bh);
    }

    // Grid: log freq lines + dB lines, 0 dB brighter.
    ctx.lineWidth = 1;
    for (const f of GRID_F) {
      const x = freqToT(f) * w;
      ctx.strokeStyle = 'rgba(148,163,184,0.18)';
      ctx.beginPath();
      ctx.moveTo(x, 0);
      ctx.lineTo(x, h);
      ctx.stroke();
    }
    ctx.font = '9px system-ui';
    ctx.fillStyle = 'rgba(154,164,178,0.8)';
    for (const f of [100, 1000, 10000]) ctx.fillText(fmtFreq(f), freqToT(f) * w + 3, 10);
    for (let db = -GSHOW; db <= GSHOW; db += 5) {
      const y = yOf(db, h);
      ctx.strokeStyle = db === 0 ? 'rgba(148,163,184,0.45)' : 'rgba(148,163,184,0.14)';
      ctx.beginPath();
      ctx.moveTo(0, y);
      ctx.lineTo(w, y);
      ctx.stroke();
      if (db !== 0) ctx.fillText(`${db > 0 ? '+' : ''}${db}`, 3, y - 2);
    }

    // Per-band faint curves + summed curve.
    ctx.lineWidth = 1;
    for (const i of visIdx) {
      const b = bands[i];
      if (!b) continue;
      ctx.strokeStyle = `${COLORS[i % COLORS.length]}55`;
      ctx.beginPath();
      for (let s = 0; s <= 90; s++) {
        const f = tToFreq(s / 90);
        const y = yOf(bandDb(b, f), h);
        if (s === 0) ctx.moveTo(0, y);
        else ctx.lineTo((s / 90) * w, y);
      }
      ctx.stroke();
    }
    ctx.lineWidth = 2;
    ctx.strokeStyle = '#e2e8f0';
    ctx.beginPath();
    for (let s = 0; s <= 120; s++) {
      const f = tToFreq(s / 120);
      const y = Math.min(h, Math.max(0, yOf(curveDb(bands, f), h)));
      if (s === 0) ctx.moveTo(0, y);
      else ctx.lineTo((s / 120) * w, y);
    }
    ctx.stroke();

    // Balls.
    for (const i of visIdx) {
      const b = bands[i];
      if (!b) continue;
      const p = bandPoint(b, w, h);
      const c = COLORS[i % COLORS.length] ?? '#fff';
      ctx.beginPath();
      ctx.arc(p.x, p.y, i === selSafe ? 10 : 8, 0, Math.PI * 2);
      ctx.fillStyle = c;
      ctx.fill();
      ctx.lineWidth = i === selSafe ? 3 : 1.5;
      ctx.strokeStyle = '#0b0f14';
      ctx.stroke();
      if (i === selSafe) {
        ctx.beginPath();
        ctx.arc(p.x, p.y, 12, 0, Math.PI * 2);
        ctx.lineWidth = 1.5;
        ctx.strokeStyle = '#fff';
        ctx.stroke();
      }
      ctx.fillStyle = '#0b0f14';
      ctx.font = 'bold 9px system-ui';
      ctx.fillText(`${i + 1}`, p.x - 3, p.y + 3);
    }
  }

  /* ---------- interaction ---------- */

  const pointers = new Map<number, { x: number; y: number }>();
  let dragId: number | null = null;
  let pinch0: { dist: number; q: number } | null = null;
  let lastTap = { n: -1, t: 0 };

  function pos(e: PointerEvent): { x: number; y: number } {
    const r = (e.target as Element).getBoundingClientRect?.() ?? canvas?.getBoundingClientRect();
    const rect = r as DOMRect;
    return { x: e.clientX - rect.left, y: e.clientY - rect.top };
  }

  function nearestBand(x: number, y: number, maxDist: number): number {
    const cv = canvas;
    if (!cv || !cv.clientWidth) return selSafe;
    let best = selSafe;
    let bd = maxDist;
    for (const i of visIdx) {
      const b = bands[i];
      if (!b) continue;
      const p = bandPoint(b, cv.clientWidth, cv.clientHeight);
      const d = Math.hypot(p.x - x, p.y - y);
      if (d < bd) {
        bd = d;
        best = i;
      }
    }
    return best;
  }

  function onPointerDown(e: PointerEvent): void {
    const cv = canvas;
    if (!cv) return;
    cv.setPointerCapture(e.pointerId);
    const p = pos(e);
    pointers.set(e.pointerId, p);
    if (pointers.size === 2) {
      // Second finger: switch from drag to pinch-Q on the selected band.
      const [a, b] = [...pointers.values()];
      if (a && b) {
        const sb = bands[selSafe];
        pinch0 = { dist: Math.max(8, Math.hypot(a.x - b.x, a.y - b.y)), q: sb?.q ?? 1 };
      }
      dragId = null;
      return;
    }
    const hit = nearestBand(p.x, p.y, 22);
    const hb = bands[hit];
    if (hb) {
      const bp = bandPoint(hb, cv.clientWidth, cv.clientHeight);
      if (Math.hypot(bp.x - p.x, bp.y - p.y) <= 22) {
        sel = hit;
        dragId = e.pointerId;
        return;
      }
    }
    // Tap elsewhere: select nearest band (wheel/Q target) without dragging.
    sel = nearestBand(p.x, p.y, Infinity);
  }

  function onPointerMove(e: PointerEvent): void {
    const p = pos(e);
    pointers.set(e.pointerId, p);
    const cv = canvas;
    if (!cv) return;
    if (pinch0 && pointers.size === 2) {
      const [a, b] = [...pointers.values()];
      if (a && b) {
        const d = Math.max(8, Math.hypot(a.x - b.x, a.y - b.y));
        const sb = bands[selSafe];
        // Spread = wider = lower Q.
        if (sb) sb.q = clampQ(pinch0.q * (pinch0.dist / d));
      }
      return;
    }
    if (dragId !== e.pointerId) return;
    const b = bands[selSafe];
    if (!b) return;
    b.freq = clampFreq(tToFreq(p.x / cv.clientWidth));
    b.gain = clampGain(GSHOW - (p.y / cv.clientHeight) * 2 * GSHOW);
  }

  function endPointer(e: PointerEvent): void {
    pointers.delete(e.pointerId);
    if (pinch0 && pointers.size < 2) {
      pinch0 = null;
      push(selSafe); // pinch-Q lands on release
    }
    if (dragId === e.pointerId) {
      dragId = null;
      push(selSafe); // drag lands on release (no relay spam mid-drag)
      if (e.pointerType !== 'mouse') {
        const now = performance.now();
        if (lastTap.n === selSafe && now - lastTap.t < 350) {
          resetBand(selSafe); // double-tap = reset band
          lastTap = { n: -1, t: 0 };
        } else {
          lastTap = { n: selSafe, t: now };
        }
      }
    }
  }

  function onDblClick(e: MouseEvent): void {
    const cv = canvas;
    if (!cv) return;
    const r = cv.getBoundingClientRect();
    resetBand(nearestBand(e.clientX - r.left, e.clientY - r.top, 28));
  }

  function onWheelNative(e: WheelEvent): void {
    e.preventDefault();
    const b = bands[selSafe];
    if (!b) return;
    b.q = clampQ(b.q * Math.pow(1.0015, e.deltaY)); // scroll up = narrower
    window.clearTimeout(wheelTimer);
    wheelTimer = window.setTimeout(() => push(selSafe), 250);
  }

  function onKey(e: KeyboardEvent): void {
    const b = bands[selSafe];
    if (!b) return;
    const cv = canvas;
    if (!cv) return;
    if (e.key >= '1' && e.key <= '4') {
      const i = Number(e.key) - 1;
      if (visIdx.includes(i)) {
        sel = i;
        e.preventDefault();
      }
      return;
    }
    const stepT = e.shiftKey ? 0.005 : 0.02;
    if (e.key === 'ArrowUp') {
      b.gain = clampGain(b.gain + (e.shiftKey ? 0.1 : 0.5));
    } else if (e.key === 'ArrowDown') {
      b.gain = clampGain(b.gain - (e.shiftKey ? 0.1 : 0.5));
    } else if (e.key === 'ArrowRight') {
      b.freq = clampFreq(tToFreq(freqToT(b.freq) + stepT));
    } else if (e.key === 'ArrowLeft') {
      b.freq = clampFreq(tToFreq(freqToT(b.freq) - stepT));
    } else if (e.key === '+' || e.key === '=') {
      b.q = clampQ(b.q * 1.12);
    } else if (e.key === '-' || e.key === '_') {
      b.q = clampQ(b.q / 1.12);
    } else {
      return;
    }
    e.preventDefault();
    push(selSafe);
  }

  onMount(() => {
    raf = requestAnimationFrame(paint);
    // Non-passive so wheel-Q can preventDefault the page scroll.
    canvas?.addEventListener('wheel', onWheelNative, { passive: false });
  });

  onDestroy(() => {
    cancelAnimationFrame(raf);
    window.clearTimeout(wheelTimer);
    canvas?.removeEventListener('wheel', onWheelNative);
  });
</script>

{#if !visIdx.length}
  <div class="hint">EQ unsupported on this backend (all eq bands rejected).</div>
{:else}
  {@const b = bands[selSafe]}
  <div class="eqwrap">
    <canvas
      bind:this={canvas}
      class="eq"
      tabindex="0"
      aria-label={b
        ? `EQ graph, band ${selSafe + 1} selected: ${fmtFreq(b.freq)} Hz, ${b.gain.toFixed(1)} dB, Q ${b.q.toFixed(1)}. Arrows move the band, plus and minus change Q, keys 1 to 4 select.`
        : 'EQ graph'}
      onpointerdown={onPointerDown}
      onpointermove={onPointerMove}
      onpointerup={endPointer}
      onpointercancel={endPointer}
      ondblclick={onDblClick}
      onkeydown={onKey}
    ></canvas>
    {#if !(spectrum && spectrum.length)}
      <span class="tag">RTA fallback</span>
    {/if}
  </div>
  {#if b}
    <div class="eread" aria-live="polite">
      <span class="dot" style={`background: ${COLORS[selSafe % COLORS.length]}`}></span>
      <b>B{selSafe + 1}</b>
      <span>{fmtFreq(b.freq)}Hz · {b.gain >= 0 ? '+' : ''}{b.gain.toFixed(1)}dB · Q{b.q.toFixed(1)}</span>
      <span class="sp"></span>
      {#each visIdx as i (i)}
        <button class="bn" class:on={i === selSafe} onclick={() => { sel = i; }} title={`select band ${i + 1}`}>{i + 1}</button>
      {/each}
      <button class="rst" onclick={() => resetBand(selSafe)} title="reset band (double-tap ball does this too)">reset</button>
    </div>
    <div class="hint">drag ball = gain/freq · wheel = Q · pinch = Q · double-tap = reset</div>
  {/if}
{/if}

<style>
  .eqwrap {
    position: relative;
    height: 180px;
    background: #0b0f14;
    border-radius: 6px;
    overflow: hidden;
  }
  .eq {
    width: 100%;
    height: 100%;
    display: block;
    touch-action: none;
    cursor: crosshair;
  }
  .eq:focus-visible {
    outline: 2px solid #38bdf8;
    outline-offset: -2px;
  }
  .tag {
    position: absolute;
    right: 4px;
    top: 2px;
    font-size: 10px;
    opacity: 0.55;
    color: #9aa4b2;
  }
  .eread {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 2px 0;
    font-size: 12px;
  }
  .dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
  }
  .sp { flex: 1; }
  .bn, .rst {
    font: inherit;
    font-size: 11px;
    padding: 1px 7px;
    border-radius: 4px;
  }
  .bn.on { background: #333; color: #fff; }
</style>

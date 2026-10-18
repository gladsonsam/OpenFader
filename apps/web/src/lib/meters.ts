// Imperative canvas meter painting, driven by the rAF loop in App.svelte.
// Kept outside components so strip + bridge painting stays identical.
//
// Hot-path rules (60fps): nothing here allocates per frame — hoisted
// constants, cached 2d contexts, indexed loops (no forEach closures, no
// array literals, no string building), and early-outs for hidden tabs,
// offscreen strips, and unchanged values.

/** Grid-line fractions shared by all vertical meters (hoisted: not per-frame). */
const GRID_FRACTIONS = [0.75, 0.5, 0.25] as const;

/**
 * Backing-store DPR cap: above 2x costs fill-rate without visible crispness
 * gain on 16px-wide meters. Display size stays CSS-pinned (16x190).
 */
const MAX_DPR = 2;

function barColor(v: number): string {
  return v > 0.95 ? '#e5484d' : v > 0.75 ? '#f5b83f' : '#46c46a';
}

/* ---------- cached 2d contexts (getContext once per canvas) ---------- */

const ctxCache = new WeakMap<HTMLCanvasElement, CanvasRenderingContext2D | null>();

function ctxFor(cv: HTMLCanvasElement): CanvasRenderingContext2D | null {
  const hit = ctxCache.get(cv);
  if (hit !== undefined) return hit;
  const ctx = cv.getContext('2d') as CanvasRenderingContext2D | null;
  ctxCache.set(cv, ctx);
  return ctx;
}

/* ---------- visibility: Strip feeds an IntersectionObserver here ---------- */

const hiddenMeters = new Set<HTMLCanvasElement>();

/** Called from Strip's IntersectionObserver; paint skips hidden canvases. */
export function setMeterVisible(cv: HTMLCanvasElement, visible: boolean): void {
  if (visible) hiddenMeters.delete(cv);
  else hiddenMeters.add(cv);
}

/* ---------- DPR-capped backing store (mount-time only, never per-frame) ---------- */

/**
 * Size a meter canvas backing store for the display DPR, capped at MAX_DPR.
 * No-op when already sized. CSS keeps the displayed size unchanged.
 */
export function sizeMeterCanvas(cv: HTMLCanvasElement, cssW: number, cssH: number): void {
  const dpr = Math.min(window.devicePixelRatio || 1, MAX_DPR);
  const w = Math.max(1, Math.round(cssW * dpr));
  const h = Math.max(1, Math.round(cssH * dpr));
  if (cv.width !== w || cv.height !== h) {
    cv.width = w;
    cv.height = h;
  }
}

/* ---------- last-frame cache: skip repaints with identical inputs ---------- */

interface StageKey {
  i: number;
  o: number;
  c: number;
  w: number;
  h: number;
  p: number;
}

const stageCache = new WeakMap<HTMLCanvasElement, StageKey>();

/* Peak-hold state per canvas (decays ~1.5 dB/s). */
const peakState = new WeakMap<HTMLCanvasElement, { peak: number; t: number }>();

interface BarKey {
  v: number;
  w: number;
  h: number;
}

const barCache = new WeakMap<HTMLCanvasElement, BarKey>();

/* ---------- dev-only perf overlay (?perf=1, no dependencies) ---------- */

let perfInit = false;
let perfOn = false;
let perfEl: HTMLDivElement | null = null;
let perfPaints = 0;
let perfPaintMs = 0;
let perfWindowStart = 0;
let perfCanvases: WeakSet<HTMLCanvasElement> | null = null;
let perfCanvasCount = 0;

/**
 * Mount a tiny fps + frame-ms overlay when `?perf=1` is set in a dev build.
 * Called once per strip mount; inert everywhere else (release, no flag).
 */
export function ensurePerfOverlay(): void {
  if (perfInit) return;
  perfInit = true;
  let dev = false;
  try {
    dev = !!import.meta.env?.DEV;
  } catch {
    dev = false;
  }
  if (!dev) return;
  let flag = false;
  try {
    flag = /([?&])perf=1(&|$)/.test(window.location.search);
  } catch {
    flag = false;
  }
  if (!flag) return;
  perfOn = true;
  perfWindowStart = performance.now();
  perfCanvases = new WeakSet();
  const el = document.createElement('div');
  el.id = 'meter-perf';
  el.style.position = 'fixed';
  el.style.left = '8px';
  el.style.bottom = '8px';
  el.style.zIndex = '9999';
  el.style.font = '11px/1.5 monospace';
  el.style.color = '#0f0';
  el.style.background = 'rgba(0,0,0,.7)';
  el.style.padding = '4px 8px';
  el.style.borderRadius = '4px';
  el.style.pointerEvents = 'none';
  el.textContent = 'meter perf…';
  document.body.appendChild(el);
  perfEl = el;
}

function perfSample(cv: HTMLCanvasElement, ms: number): void {
  if (!perfOn) return;
  let seen = perfCanvases;
  if (!seen) {
    seen = new WeakSet();
    perfCanvases = seen;
  }
  if (!seen.has(cv)) {
    seen.add(cv);
    perfCanvasCount += 1;
  }
  perfPaints += 1;
  perfPaintMs += ms;
  const now = performance.now();
  const span = now - perfWindowStart;
  if (span >= 500) {
    const perFrame = perfPaints / Math.max(1, perfCanvasCount);
    const fps = (perFrame * 1000) / span;
    const avg = perfPaintMs / Math.max(1, perfPaints);
    if (perfEl) perfEl.textContent = `meters ~${fps.toFixed(0)}fps · ${avg.toFixed(2)}ms/paint · ${perfCanvasCount} strips`;
    perfPaints = 0;
    perfPaintMs = 0;
    perfCanvasCount = 0;
    perfCanvases = new WeakSet();
    perfWindowStart = now;
  }
}

/** Null when painting should be skipped (hidden tab / offscreen strip). */
function paintCtx(cv: HTMLCanvasElement): CanvasRenderingContext2D | null {
  if (typeof document !== 'undefined' && document.hidden) return null;
  if (hiddenMeters.has(cv)) return null;
  return ctxFor(cv);
}

export function paintMeter(cv: HTMLCanvasElement, v: number): void {
  const t0 = perfOn ? performance.now() : 0;
  const ctx = paintCtx(cv);
  if (!ctx) return;
  const W = cv.width;
  const H = cv.height;
  const hit = barCache.get(cv);
  if (hit !== undefined && hit.v === v && hit.w === W && hit.h === H) return;
  if (hit === undefined) barCache.set(cv, { v, w: W, h: H });
  else {
    hit.v = v;
    hit.w = W;
    hit.h = H;
  }
  ctx.clearRect(0, 0, W, H);
  const h = Math.max(1, v * H);
  ctx.fillStyle = barColor(v);
  ctx.fillRect(0, H - h, W, h);
  ctx.fillStyle = 'rgba(255,255,255,.25)';
  for (let k = 0; k < GRID_FRACTIONS.length; k++) {
    const f = GRID_FRACTIONS[k];
    ctx.fillRect(0, H * (1 - f), W, 1);
  }
  if (perfOn) perfSample(cv, performance.now() - t0);
}

export function paintBridge(cv: HTMLCanvasElement, frame: ReadonlyArray<number>): void {
  // Minimal documented cast: 2d context is always available on these canvases.
  const t0 = perfOn ? performance.now() : 0;
  const ctx = paintCtx(cv);
  if (!ctx) return;
  const W = cv.width;
  const H = cv.height;
  const n = frame.length;
  if (!n) return;
  const w = W / n;
  ctx.clearRect(0, 0, W, H);
  for (let i = 0; i < n; i++) {
    const v = frame[i];
    const h = Math.max(1, v * (H - 4));
    ctx.fillStyle = barColor(v);
    ctx.fillRect(i * w + 2, H - 2 - h, w - 4, h);
  }
  if (perfOn) perfSample(cv, performance.now() - t0);
}

/**
 * Staging meter: pre-fader input estimate behind (full-width, dim) with the
 * post-fader output in front (narrow, bright) + latched clip LED on top.
 * `input` is a pre-fader estimate (level / fader) until the relay sends
 * real pre/post frames — see V2.md. Tap the meter to clear a latched clip.
 */
export function paintStageMeter(
  cv: HTMLCanvasElement,
  input: number,
  output: number,
  clip: boolean,
): void {
  // Minimal documented cast: 2d context is always available on these canvases.
  const t0 = perfOn ? performance.now() : 0;
  const ctx = paintCtx(cv);
  if (!ctx) return;
  const W = cv.width;
  const H = cv.height;
  const c = clip ? 1 : 0;
  // Peak hold: decay ~1.5 dB/s (~0.04/s in linear).
  const now = performance.now();
  let ps = peakState.get(cv);
  if (!ps) { ps = { peak: output, t: now }; peakState.set(cv, ps); }
  if (output >= ps.peak) { ps.peak = output; ps.t = now; }
  else {
    const dt = (now - ps.t) / 1000;
    ps.peak = Math.max(output, ps.peak - dt * 0.04);
    ps.t = now;
  }
  const peak = ps.peak;
  const hit = stageCache.get(cv);
  if (hit !== undefined && hit.i === input && hit.o === output && hit.c === c && hit.p === peak && hit.w === W && hit.h === H)
    return;
  if (hit === undefined) stageCache.set(cv, { i: input, o: output, c, p: peak, w: W, h: H });
  else { hit.i = input; hit.o = output; hit.c = c; hit.p = peak; hit.w = W; hit.h = H; }
  ctx.clearRect(0, 0, W, H);
  const inH = Math.max(1, Math.min(1, input) * (H - 8));
  ctx.fillStyle = barColor(Math.min(1, input));
  ctx.globalAlpha = 0.38;
  ctx.fillRect(0, H - inH, W, inH);
  ctx.globalAlpha = 1;
  const outH = Math.max(1, Math.min(1, output) * (H - 8));
  ctx.fillStyle = barColor(Math.min(1, output));
  const cw = Math.max(3, W * 0.45);
  ctx.fillRect((W - cw) / 2, H - outH, cw, outH);
  // Peak-hold line.
  const pH = Math.max(1, Math.min(1, peak) * (H - 8));
  ctx.fillStyle = '#fff';
  ctx.globalAlpha = 0.85;
  ctx.fillRect(0, H - pH, W, 1.5);
  ctx.globalAlpha = 1;
  ctx.fillStyle = 'rgba(255,255,255,.25)';
  for (let k = 0; k < GRID_FRACTIONS.length; k++) {
    const f = GRID_FRACTIONS[k];
    ctx.fillRect(0, H * (1 - f), W, 1);
  }
  // Clip latch LED: bright red cap while latched.
  ctx.fillStyle = clip ? '#ff2d2d' : 'rgba(255,45,45,.18)';
  ctx.fillRect(0, 0, W, 5);
  if (perfOn) perfSample(cv, performance.now() - t0);
}

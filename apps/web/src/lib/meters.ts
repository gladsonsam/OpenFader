// Imperative canvas meter painting, driven by the rAF loop in App.svelte.
// Kept outside components so strip + bridge painting stays identical.

function barColor(v: number): string {
  return v > 0.95 ? '#e5484d' : v > 0.75 ? '#f5b83f' : '#46c46a';
}

export function paintMeter(cv: HTMLCanvasElement, v: number): void {
  // Minimal documented cast: 2d context is always available on these canvases.
  const ctx = cv.getContext('2d') as CanvasRenderingContext2D | null;
  if (!ctx) return;
  const W = cv.width;
  const H = cv.height;
  ctx.clearRect(0, 0, W, H);
  const h = Math.max(1, v * H);
  ctx.fillStyle = barColor(v);
  ctx.fillRect(0, H - h, W, h);
  ctx.fillStyle = 'rgba(255,255,255,.25)';
  for (const f of [0.75, 0.5, 0.25]) ctx.fillRect(0, H * (1 - f), W, 1);
}

export function paintBridge(cv: HTMLCanvasElement, frame: ReadonlyArray<number>): void {  // Minimal documented cast: 2d context is always available on these canvases.
  const ctx = cv.getContext('2d') as CanvasRenderingContext2D | null;
  if (!ctx) return;
  const W = cv.width;
  const H = cv.height;
  const n = frame.length;
  if (!n) return;
  const w = W / n;
  ctx.clearRect(0, 0, W, H);
  frame.forEach((v, i) => {
    const h = Math.max(1, v * (H - 4));
    ctx.fillStyle = barColor(v);
    ctx.fillRect(i * w + 2, H - 2 - h, w - 4, h);
  });
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
  const ctx = cv.getContext('2d') as CanvasRenderingContext2D | null;
  if (!ctx) return;
  const W = cv.width;
  const H = cv.height;
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
  ctx.fillStyle = 'rgba(255,255,255,.25)';
  for (const f of [0.75, 0.5, 0.25]) ctx.fillRect(0, H * (1 - f), W, 1);
  // Clip latch LED: bright red cap while latched.
  ctx.fillStyle = clip ? '#ff2d2d' : 'rgba(255,45,45,.18)';
  ctx.fillRect(0, 0, W, 5);
}

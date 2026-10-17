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

export function paintBridge(cv: HTMLCanvasElement, frame: ReadonlyArray<number>): void {
  // Minimal documented cast: 2d context is always available on these canvases.
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

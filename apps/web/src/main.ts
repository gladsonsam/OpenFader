const target = document.getElementById('app');
if (!target) throw new Error('#app mount target not found');

// ui4 is the only UI. `/` mounts it directly; `?ui=4` remains as an alias.
// Fixture/golden query params (?fixture=1&freeze=1&screen=...&gallery=...)
// are honoured by the ui4 mount for the design-diff harness in
// apps/web/tools/design-diff/.
const params = new URLSearchParams(window.location.search);

async function boot(el: HTMLElement) {
  await import('./ui/tokens.css');
  await import('./ui/app.css');
  // Native mobile bridge (edge-to-edge, wake-lock, logging). No-op in
  // browser/PWA; never blocks first paint.
  import('./mobile.ts').then((m) => m.initMobile()).catch(() => undefined);
  const m = await import('./ui/mount.ts');
  return m.mountUi(el, params);
}

const app = boot(target);

export default app;

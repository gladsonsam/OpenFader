import { createStartupView, startupPaint, StartupTimeout } from './ui/startup.ts';

const target = document.getElementById('app');
if (!target) throw new Error('#app mount target not found');
// Keep a useful branded view on screen throughout local code preparation.
// mountUi replaces it only after every normal navigation destination is ready.
const startup = createStartupView(target);

// ui4 is the only UI. `/` mounts it directly; `?ui=4` remains as an alias.
// Fixture query params (?fixture=1&freeze=1&screen=...&gallery=...)
// are honoured by the ui4 mount (current UI + screenshots are the baseline).
const params = new URLSearchParams(window.location.search);

async function boot(el: HTMLElement, signal: AbortSignal) {
  // Tokens + app CSS fetch in parallel (serial awaits stalled first paint
  // on the slower of the two).
  await Promise.all([import('./ui/tokens.css'), import('./ui/app.css'), startupPaint()]);
  signal.throwIfAborted();
  // App-shell service worker is a production/PWA concern. Registered
  // during dev it serves stale-while-revalidate transforms, so the
  // engineer tests old code after every edit. Never register it here;
  // drop any registration left over from a production-like load.
  if (import.meta.env.DEV && 'serviceWorker' in navigator) {
    navigator.serviceWorker
      .getRegistrations()
      .then((rs) => {
        for (const r of rs) r.unregister().catch(() => undefined);
      })
      .catch(() => undefined);
  }
  if (import.meta.env.PROD && 'serviceWorker' in navigator) {
    window.addEventListener('load', () => {
      navigator.serviceWorker.register('./sw.js').catch(() => {});
    });
  }
  // Native mobile bridge (wake-lock, logging). No-op in
  // browser/PWA; never blocks first paint.
  import('./mobile.ts').then((m) => m.initMobile()).catch(() => undefined);
  // Background battery: pause canvas painting while hidden (the push
  // stream stays live). Works in browser/PWA too — not Tauri-gated.
  import('./mobile.ts').then((m) => m.initAppVisibility()).catch(() => undefined);
  const m = await import('./ui/mount.ts');
  signal.throwIfAborted();
  return m.mountUi(el, params, {
    signal,
    onProgress: (completed, total) => {
      if (!signal.aborted) startup.progress(completed, total);
    },
  });
}

async function start(): Promise<object> {
  const controller = new AbortController();
  startup.loading();
  performance.mark('openmix:startup-start');
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    const timeout = new Promise<never>((_, reject) => {
      timer = setTimeout(() => {
        const error = new StartupTimeout();
        controller.abort(error);
        reject(error);
      }, 25_000);
    });
    const app = await Promise.race([boot(target!, controller.signal), timeout]);
    target!.dataset.startupState = 'ready';
    performance.mark('openmix:app-ready');
    performance.measure('openmix:startup', 'openmix:startup-start', 'openmix:app-ready');
    return app;
  } catch (error) {
    clearTimeout(timer);
    controller.abort(error);
    console.error('[openmix] local UI preparation failed:', error);
    // A settled failure or a stalled import never leaves an endless spinner.
    // Timed-out imports may settle in cache, but cannot mount after cancellation.
    return await new Promise<object>(resolve => {
      startup.failed(error, () => {
        // Disable repeated clicks before scheduling another bounded attempt.
        startup.loading();
        resolve(start());
      });
    });
  } finally {
    clearTimeout(timer);
  }
}

const app = start();

export default app;

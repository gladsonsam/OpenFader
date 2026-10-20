/* openmix service worker — app-shell cache, socket traffic never cached.
 *
 * Strategy:
 *  - /ws + API requests: network-only, never cached (live socket traffic).
 *  - navigations: network-first, then cache, then offline fallback page.
 *  - same-origin GET assets (dist js/css, icons, manifest): stale-while-revalidate.
 *  - everything else: pass through.
 */

const CACHE = 'openmix-shell-v5';
const CORE = ['./', './index.html', './manifest.webmanifest'];

function isSocketOrApi(url) {
  const path = url.pathname;
  return (
    path === '/ws' ||
    path.startsWith('/ws/') ||
    path.startsWith('/api/') ||
    path.startsWith('/socket')
  );
}

self.addEventListener('install', (event) => {
  event.waitUntil(
    caches
      .open(CACHE)
      .then((cache) => cache.addAll(CORE))
      .then(() => self.skipWaiting()),
  );
});

self.addEventListener('activate', (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((keys) =>
        Promise.all(keys.filter((k) => k.startsWith('openmix-shell-') && k !== CACHE).map((k) => caches.delete(k))),
      )
      .then(() => self.clients.claim()),
  );
});

function offlineFallback() {
  const html = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<meta name="theme-color" content="#0b0e14" />
<title>openmix — offline</title>
<style>
  body { margin: 0; min-height: 100vh; display: grid; place-items: center;
         background: #0b0e14; color: #e5e7eb;
         font-family: system-ui, -apple-system, sans-serif; }
  main { text-align: center; padding: 2rem; max-width: 26rem; }
  h1 { font-size: 1.5rem; margin: 0 0 0.5rem; }
  code { color: #2dd4bf; }
  button { margin-top: 1.25rem; padding: 0.6rem 1.4rem; font-size: 1rem;
           border: 0; border-radius: 0.5rem; background: #2dd4bf; color: #0b0e14; }
</style>
</head>
<body>
<main>
  <h1>openmix is offline</h1>
  <p><code>connect to openmix-serve</code> on the gig network, then retry.</p>
  <p>Tip: pass the server explicitly, e.g.
     <code>?server=192.168.1.10:3000</code></p>
  <button onclick="location.reload()">retry</button>
</main>
<script>
  // Keep the ?server= hint across the retry.
  document.querySelector('button').addEventListener('click', () => {
    const q = new URLSearchParams(location.search);
    if (!q.get('server')) q.set('server', '192.168.1.10:3000');
    location.search = q.toString();
  });
</script>
</body>
</html>`;
  return new Response(html, {
    status: 200,
    headers: { 'Content-Type': 'text/html; charset=utf-8' },
  });
}

self.addEventListener('fetch', (event) => {
  const { request } = event;
  if (request.method !== 'GET') return;
  const url = new URL(request.url);
  if (url.protocol !== 'http:' && url.protocol !== 'https:') return;

  // Live mixer traffic — never cache, never serve stale.
  if (isSocketOrApi(url)) return;

  // Navigations: network-first so a fresh deploy wins on gig wifi,
  // cache second, offline fallback page last.
  if (request.mode === 'navigate') {
    event.respondWith(
      fetch(request)
        .then((res) => {
          const copy = res.clone();
          caches.open(CACHE).then((cache) => cache.put('./index.html', copy));
          return res;
        })
        .catch(() =>
          caches.match('./index.html').then((hit) => hit || offlineFallback()),
        ),
    );
    return;
  }

  // Same-origin static assets (hashed dist js/css, icons, manifest):
  // serve cached copy fast, refresh in background.
  if (url.origin === self.location.origin) {
    event.respondWith(
      caches.match(request).then((hit) => {
        const refresh = fetch(request)
          .then((res) => {
            if (res.ok) {
              const copy = res.clone();
              caches.open(CACHE).then((cache) => cache.put(request, copy));
            }
            return res;
          })
          .catch(() => hit);
        return hit || refresh;
      }),
    );
  }
});

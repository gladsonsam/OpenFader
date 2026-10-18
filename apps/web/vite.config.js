import { defineConfig } from 'vite';
import { svelte, vitePreprocess } from '@sveltejs/vite-plugin-svelte';
import { cpSync, mkdirSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const webRoot = dirname(fileURLToPath(import.meta.url));

// PWA wiring only: Vite emits bundled assets + public/, so copy the
// installability sources (manifest, service worker, icons) into dist
// verbatim at the end of the build.
function pwaAssets() {
  let outDir = 'dist';
  return {
    name: 'openmix-pwa-assets',
    configResolved(config) {
      outDir = config.build.outDir;
    },
    closeBundle() {
      const out = resolve(webRoot, outDir);
      cpSync(resolve(webRoot, 'manifest.webmanifest'), resolve(out, 'manifest.webmanifest'));
      cpSync(resolve(webRoot, 'sw.js'), resolve(out, 'sw.js'));
      mkdirSync(resolve(out, 'icons'), { recursive: true });
      cpSync(resolve(webRoot, 'icons'), resolve(out, 'icons'), { recursive: true });
    },
  };
}

// Static output only (no SSR): dist/ is served as plain files,
// by `vite preview`, any static file server, or the Tauri shell.
export default defineConfig({
  plugins: [svelte({ preprocess: vitePreprocess() }), pwaAssets()],
  base: './',
  // Dev/preview behind the dev1-<port>.gladsonsam.com tunnel (host check would block it).
  server: { allowedHosts: ['.gladsonsam.com'] },
  preview: { allowedHosts: ['.gladsonsam.com'] },
  build: {
    outDir: 'dist',
    emptyOutDir: true,
  },
});

import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

// Static output only (no SSR): dist/ is served as plain files,
// by `vite preview`, any static file server, or the Tauri shell.
export default defineConfig({
  plugins: [svelte()],
  base: './',
  build: {
    outDir: 'dist',
    emptyOutDir: true,
  },
});

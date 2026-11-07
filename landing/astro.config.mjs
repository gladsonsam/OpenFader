import { defineConfig } from 'astro/config';
import docShots from './src/lib/rehype-doc-shots.mjs';

export default defineConfig({
  site: 'https://openfader.app',
  markdown: { rehypePlugins: [docShots] },
  outDir: './dist',
  trailingSlash: 'never',
  vite: {
    server: {
      allowedHosts: ['dev-4311.gladsonsam.com', 'localhost', '127.0.0.1'],
    },
    preview: {
      allowedHosts: ['dev-4311.gladsonsam.com', 'localhost', '127.0.0.1'],
    },
  },
});

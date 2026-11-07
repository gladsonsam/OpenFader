import { defineConfig } from 'astro/config';

export default defineConfig({
  site: 'https://openfader.example.com',
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

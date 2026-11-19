import { svelte } from '@sveltejs/vite-plugin-svelte';
import { defineConfig } from 'vite';

// Built into dist/, which `harness/seed.mjs` publishes as the group's site,
// `index.html` its fallback for every room's address. Its scripts and
// styles are files under /assets/, served at their own paths.
export default defineConfig({
  plugins: [svelte({ compilerOptions: { runes: true } })],
  build: { outDir: 'dist', emptyOutDir: true }
});

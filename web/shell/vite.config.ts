// The page around a group's site that asks for live data, built on its
// own into build-shell/shell.js, which the host serves at /_os/shell.js
// on a group's address (SITE_SHELL_DIR). It draws the console's own
// sign-in dialog, so it is built with the console's Svelte and styles.
import { fileURLToPath } from 'node:url';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { defineConfig } from 'vite';
import { paraglide } from '../paraglide.ts';

const here = fileURLToPath(new URL('.', import.meta.url));

export default defineConfig({
  root: here,
  // The console's messages, for the words the sign-in dialog says.
  plugins: [paraglide(), svelte({ compilerOptions: { runes: true } })],
  // @opensocial/sdk is linked from packages/sdk, with packages of its own:
  // one copy of lex here, so that its errors are the classes this page
  // tells apart.
  resolve: { dedupe: ['@atproto/lex', '@atproto/lex-space'] },
  build: {
    outDir: '../build-shell',
    emptyOutDir: true,
    target: 'es2022',
    lib: { entry: 'shell.ts', formats: ['es'], fileName: () => 'shell.js' }
  }
});

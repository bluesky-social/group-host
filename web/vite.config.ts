import adapter from '@sveltejs/adapter-node';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

/** The group host, which `vite dev` hands what it answers itself. */
const host = process.env.GROUP_HOST ?? 'http://127.0.0.1:2600';
/**
 * Where people reach the console when not at the dev server itself, as
 * the built console reads it too: `just dev` serves it behind portless at
 * https://group-host.localtest.me. The dev server answers that name, takes
 * a form's post from it, and has the page find its updates through it.
 */
const origin = process.env.ORIGIN ? new URL(process.env.ORIGIN) : null;

export default defineConfig({
  server: {
    allowedHosts: origin ? [origin.hostname] : [],
    hmr: origin
      ? { clientPort: Number(origin.port || (origin.protocol === 'https:' ? 443 : 80)) }
      : undefined,
    proxy: { '^/xrpc/': host, '^/oauth/': host, '^/\\.well-known/': host, '^/img/': host }
  },
  plugins: [
    sveltekit({
      compilerOptions: { runes: true },
      adapter: adapter(),
      csrf: { trustedOrigins: origin ? [origin.origin] : [] },
      // One stylesheet, small enough to come with each page.
      inlineStyleThreshold: Infinity,
      csp: {
        mode: 'hash',
        directives: {
          'default-src': ['none'],
          'img-src': ['self', 'https:', 'data:', 'blob:'],
          // Avatars take their hue from a style attribute.
          'style-src': ['unsafe-inline'],
          'script-src': ['self'],
          // Handles suggested as they are typed ask the host (lib/typeahead.ts).
          'connect-src': ['self'],
          'base-uri': ['none'],
          'form-action': ['self'],
          'frame-ancestors': ['none']
        }
      }
    })
  ]
});

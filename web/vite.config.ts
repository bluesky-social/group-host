import adapter from '@sveltejs/adapter-node';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

/** The group host, which `vite dev` hands what it answers itself. */
const host = process.env.GROUP_HOST ?? 'http://127.0.0.1:2600';

export default defineConfig({
	server: {
		proxy: { '^/xrpc/': host, '^/oauth/': host, '^/\\.well-known/': host }
	},
	plugins: [
		sveltekit({
			compilerOptions: { runes: true },
			adapter: adapter(),
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
					'connect-src': ['self'],
					'base-uri': ['none'],
					'form-action': ['self'],
					'frame-ancestors': ['none']
				}
			}
		})
	]
});

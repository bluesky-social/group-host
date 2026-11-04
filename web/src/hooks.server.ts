import type { Handle, HandleFetch } from '@sveltejs/kit/hooks';
import { GROUP_HOST } from '$app/env/private';

/**
 * Signing in and out is the group host's (`/console/*`): asked of this
 * site, it is asked of the host, cookies and all, and its answer, its
 * redirects and the cookies it sets among it, is the browser's.
 */
export const handle: Handle = async ({ event, resolve }) => {
	if (!event.url.pathname.startsWith('/console/')) return resolve(event);
	const asked = new Request(`${GROUP_HOST}${event.url.pathname}${event.url.search}`, {
		method: event.request.method,
		headers: event.request.headers,
		body: event.request.method === 'GET' || event.request.method === 'HEAD' ? undefined : await event.request.arrayBuffer(),
		redirect: 'manual'
	});
	asked.headers.set('x-forwarded-host', event.url.host);
	asked.headers.set('x-forwarded-proto', event.url.protocol.slice(0, -1));
	return globalThis.fetch(asked);
};

/**
 * Asked of this site, XRPC methods are asked of the group host directly,
 * and with the global fetch: the host can share the console's origin, as
 * it does in production, which Kit's own fetch would answer itself.
 */
export const handleFetch: HandleFetch = async ({ event, request, fetch }) => {
	const url = new URL(request.url);
	if (url.origin === event.url.origin && url.pathname.startsWith('/xrpc/')) {
		// A procedure's body read whole: Node's fetch sends no stream on.
		const body = request.method === 'GET' || request.method === 'HEAD' ? undefined : await request.arrayBuffer();
		const asked = new Request(`${GROUP_HOST}${url.pathname}${url.search}`, { method: request.method, headers: request.headers, body });
		asked.headers.set('x-forwarded-host', event.url.host);
		asked.headers.set('x-forwarded-proto', event.url.protocol.slice(0, -1));
		// Who asks, as the session cookie of someone signed in says: the
		// global fetch does not send the page's cookies itself.
		const cookie = event.request.headers.get('cookie');
		if (cookie) asked.headers.set('cookie', cookie);
		return globalThis.fetch(asked);
	}
	return fetch(request);
};

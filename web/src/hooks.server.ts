import type { HandleFetch } from '@sveltejs/kit/hooks';
import { GROUP_HOST } from '$app/env/private';

/**
 * Asked of this site, XRPC methods are asked of the group host directly,
 * and with the global fetch: the host can share the console's origin, as
 * it does in production, which Kit's own fetch would answer itself.
 */
export const handleFetch: HandleFetch = ({ event, request, fetch }) => {
	const url = new URL(request.url);
	if (url.origin === event.url.origin && url.pathname.startsWith('/xrpc/')) {
		const asked = new Request(`${GROUP_HOST}${url.pathname}${url.search}`, request);
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

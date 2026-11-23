import type { Handle, HandleFetch } from '@sveltejs/kit/hooks';
import { GROUP_HOST } from '$app/env/private';
import { getTextDirection } from '#lib/paraglide/runtime.js';
import { paraglideMiddleware } from '#lib/paraglide/server.js';

/**
 * A page of the console's own, in the reader's language: Paraglide picks
 * it (`project.inlang`, and the strategy in `vite.config.ts`) for every
 * message said while drawing it, and the document says which, and which
 * way it reads.
 */
const localized: Handle = ({ event, resolve }) =>
  paraglideMiddleware(event.request, ({ request, locale }) =>
    resolve(
      { ...event, request },
      {
        transformPageChunk: ({ html }) =>
          html.replace('%lang%', locale).replace('%dir%', getTextDirection(locale))
      }
    )
  );

/**
 * What is the group host's, asked of this site, is asked of the host,
 * cookies and all, and its answer, its redirects and the cookies it sets
 * among it, is the browser's: signing in and out (`/console/*`), and its
 * methods and OAuth, which a page asks from the browser once it is there,
 * and the images it serves for a page to draw, as `vite dev` hands them
 * on.
 */
const HOSTS = ['/console/', '/xrpc/', '/oauth/', '/.well-known/', '/img/'];

export const handle: Handle = async ({ event, resolve }) => {
  if (!HOSTS.some((p) => event.url.pathname.startsWith(p))) return localized({ event, resolve });
  const asked = new Request(`${GROUP_HOST}${event.url.pathname}${event.url.search}`, {
    method: event.request.method,
    headers: event.request.headers,
    body:
      event.request.method === 'GET' || event.request.method === 'HEAD'
        ? undefined
        : await event.request.arrayBuffer(),
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
    const body =
      request.method === 'GET' || request.method === 'HEAD'
        ? undefined
        : await request.arrayBuffer();
    const asked = new Request(`${GROUP_HOST}${url.pathname}${url.search}`, {
      method: request.method,
      headers: request.headers,
      body
    });
    asked.headers.set('x-forwarded-host', event.url.host);
    asked.headers.set('x-forwarded-proto', event.url.protocol.slice(0, -1));
    // Who asks, as the session cookie of someone signed in says: the
    // global fetch does not send the page's cookies itself.
    const cookie = event.request.headers.get('cookie');
    if (cookie) asked.headers.set('cookie', cookie);
    // Who asks, as the proxy in front named them: the host's limits count
    // by it, not by the console.
    const forwarded = event.request.headers.get('x-forwarded-for');
    if (forwarded) asked.headers.set('x-forwarded-for', forwarded);
    return globalThis.fetch(asked);
  }
  return fetch(request);
};

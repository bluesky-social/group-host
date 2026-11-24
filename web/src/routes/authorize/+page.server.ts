import type * as getAuthorization from '#lib/lexicons/org/atgrp/internal/getAuthorization.ts';
import type * as getViewer from '#lib/lexicons/org/atgrp/internal/getViewer.ts';
import type { Actions, PageServerLoad } from './$types';
import { error } from '@sveltejs/kit';
import { call, xrpc } from '#lib/data.ts';
import { m } from '#lib/paraglide/messages.js';

/**
 * An app's request to sign in as a group, which the browser was sent here
 * with: what the person signed in may sign it in as. Signed out, they are
 * asked to sign in first.
 */
export const load: PageServerLoad = async (event) => {
  const requestUri = event.url.searchParams.get('request_uri');
  if (!requestUri) error(400, m.route_authorize_no_request());
  const { actor } = await xrpc<typeof getViewer.main>(event, 'org.atgrp.internal.getViewer', {});
  if (!actor) return { requestUri, request: null, expired: false };
  // Answered already (as when the page is drawn after its form), or
  // lapsed: the page says so, or goes back to the app.
  const request = await xrpc<typeof getAuthorization.main>(
    event,
    'org.atgrp.internal.getAuthorization',
    { requestUri }
  ).catch(() => null);
  return { requestUri, request, expired: request === null };
};

/**
 * The way back to the app: approved for a group, or declined. It is the
 * page's to take the browser there, not the form's: a redirect from a form
 * on https to an app listening on http, as a native app does, is the
 * browser's to warn about, as an insecure form.
 */
export const actions: Actions = {
  approve: async (event) => {
    const form = await event.request.formData();
    const input = { requestUri: String(form.get('requestUri')), group: String(form.get('group')) };
    return call<{ redirectUrl: string }>(event, 'org.atgrp.internal.approveAuthorization', input);
  },
  decline: async (event) => {
    const form = await event.request.formData();
    const input = { requestUri: String(form.get('requestUri')) };
    return call<{ redirectUrl: string }>(event, 'org.atgrp.internal.declineAuthorization', input);
  }
};

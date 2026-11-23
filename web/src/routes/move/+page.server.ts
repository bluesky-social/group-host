import type * as getViewer from '#lib/lexicons/fyi/opensocial/getViewer.ts';
import type * as getAccountMove from '#lib/lexicons/fyi/opensocial/getAccountMove.ts';
import type * as checkImport from '#lib/lexicons/fyi/opensocial/checkImport.ts';
import type { Actions, PageServerLoad } from './$types';
import { fail, redirect } from '@sveltejs/kit';
import { act, xrpc } from '#lib/data.ts';
import { refused } from '#lib/errors.ts';

type Did = `did:${string}:${string}`;

/**
 * Who is moving an account here, the account they signed in to move if
 * any, and, until its move starts, what moving it would make of it.
 */
export const load: PageServerLoad = async (event) => {
  const { actor } = await xrpc<typeof getViewer.main>(event, 'fyi.opensocial.getViewer', {});
  if (!actor?.canCreateGroup) return { actor: actor ?? null, moving: null, plan: null };
  const moving = await xrpc<typeof getAccountMove.main>(event, 'fyi.opensocial.getAccountMove', {});
  if (!moving.did) return { actor, moving: null, plan: null };
  const did = moving.did as Did;
  const address = event.url.searchParams.get('address')?.trim() || undefined;
  const plan = moving.started
    ? null
    : await xrpc<typeof checkImport.main>(event, 'fyi.opensocial.checkImport', { did, address });
  return { actor, moving: { did, handle: moving.handle, pds: moving.pds }, plan };
};

const text = (form: FormData, key: string) => String(form.get(key) ?? '').trim();

/** Starting the move, finishing it with the emailed code, and stopping. */
export const actions: Actions = {
  start: async (event) => {
    const form = await event.request.formData();
    return act(event, 'fyi.opensocial.startAccountMove', {
      address: text(form, 'address') || undefined
    });
  },
  finish: async (event) => {
    const form = await event.request.formData();
    const res = await event.fetch('/xrpc/fyi.opensocial.finishAccountMove', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ code: text(form, 'code') })
    });
    if (!res.ok) return fail(res.status, { failed: await refused(res) });
    const done: { handle: string } = await res.json();
    redirect(303, `/g/${done.handle}`);
  },
  cancel: async (event) => act(event, 'fyi.opensocial.cancelAccountMove', {})
};

import { redirect } from '@sveltejs/kit';
import type * as checkGroupHandle from '#lib/lexicons/org/atgrp/internal/checkGroupHandle.ts';
import type { PageServerLoad } from './$types';
import { xrpc } from '#lib/data.ts';
import { listed, profileActions } from '#lib/server/settings.ts';

/**
 * Its profile, its pictures, who sees it, its handle and whether it is
 * served at a domain of its own, to one who configures it, with whether
 * a handle on a domain of its own names it still; one who may only make
 * app passwords goes to those.
 */
export const load: PageServerLoad = async (event) => {
  const view = await event.parent();
  if (!view.configures && view.passwords) redirect(303, `/g/${event.params.handle}/settings/apps`);
  const { did, handle, address } = view.group;
  const own = address !== undefined && handle !== address;
  const checked = own
    ? await xrpc<typeof checkGroupHandle.main>(event, 'org.atgrp.internal.checkGroupHandle', {
        group: did,
        handle
      })
    : null;
  return {
    listed: listed(view),
    address: address ?? handle,
    pointing: checked?.ready ?? null,
    domain: own ? handle : null,
    on: !!view.group.domain,
    host: event.url.hostname
  };
};

export const actions = profileActions;

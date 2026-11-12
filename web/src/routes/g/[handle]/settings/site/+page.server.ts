import type { PageServerLoad } from './$types';
import { siteActions } from '#lib/server/settings.ts';
import { siteOf } from '#lib/server/site.ts';

/** The group's site, where it is served, to one who configures it. */
export const load: PageServerLoad = async (event) => {
  const view = await event.parent();
  const address = view.group.address ?? view.group.handle;
  return {
    address,
    site: view.configures ? await siteOf(event, view.group.did, address) : null
  };
};

export const actions = siteActions;

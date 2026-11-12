import type { PageServerLoad } from './$types';
import { siteActions } from '#lib/server/settings.ts';
import { siteOf } from '#lib/server/site.ts';

/**
 * The group's site, where it is served, to one who configures it: at its
 * own domain when it is served there, and that domain (its handle, when
 * not its address) to move it to or back from.
 */
export const load: PageServerLoad = async (event) => {
  const view = await event.parent();
  const g = view.group;
  const address = g.address ?? g.handle;
  const served = g.siteDomain ?? address;
  return {
    address,
    served,
    domain: g.handle === address ? null : g.handle,
    on: !!g.siteDomain,
    host: event.url.hostname,
    site: view.configures ? await siteOf(event, g.did, served) : null
  };
};

export const actions = siteActions;

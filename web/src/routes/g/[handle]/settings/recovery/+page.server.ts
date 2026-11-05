import type { PageServerLoad } from './$types';
import { recoveryActions, recoveryOf } from '#lib/server/settings.ts';

/** The keys that can recover the group's identity, to one who configures it, where this host keeps them. */
export const load: PageServerLoad = async (event) => {
  const view = await event.parent();
  return { recovery: view.configures ? await recoveryOf(event, view.group.did) : null };
};

export const actions = recoveryActions;

import type { PageServerLoad } from './$types';
import { appActions, tokensOf } from '#lib/server/settings.ts';

/**
 * The apps acting as the group and what they did lately, to one who
 * configures it; its app passwords, as the layout gives them, to them
 * and to whoever may make one.
 */
export const load: PageServerLoad = async (event) => {
  const view = await event.parent();
  return { tokens: view.configures ? await tokensOf(event, view.group.did) : null };
};

export const actions = appActions;

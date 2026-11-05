import { redirect } from '@sveltejs/kit';
import type { PageServerLoad } from './$types';
import { listed, profileActions } from '#lib/server/settings.ts';

/**
 * Its profile, its pictures and who sees it, to one who configures it;
 * one who may only make app passwords goes to those.
 */
export const load: PageServerLoad = async (event) => {
  const view = await event.parent();
  if (!view.configures && view.passwords) redirect(303, `/g/${event.params.handle}/settings/apps`);
  return { listed: listed(view) };
};

export const actions = profileActions;

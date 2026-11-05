import { error } from '@sveltejs/kit';
import type { PageServerLoad } from './$types';
import { appActions, tokensOf } from '#lib/server/settings.ts';

/**
 * The apps acting as the group and what they did lately, to one who
 * configures it; its app passwords, to them and to whoever may make one.
 */
export const load: PageServerLoad = async (event) => {
	const view = await event.parent();
	if (!view.configures && !view.passwords) error(403, 'Only those who run the group see the apps acting as it.');
	return { tokens: view.configures ? await tokensOf(event, view.group.did) : null };
};

export const actions = appActions;

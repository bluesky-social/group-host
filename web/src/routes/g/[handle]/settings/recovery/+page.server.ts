import { error } from '@sveltejs/kit';
import type { PageServerLoad } from './$types';
import { recoveryActions, recoveryOf } from '#lib/server/settings.ts';

/** The keys that can recover the group's identity, to one who configures it, where this host keeps them. */
export const load: PageServerLoad = async (event) => {
	const view = await event.parent();
	if (!view.configures) error(403, 'Only those who run the group see its recovery keys.');
	return { recovery: await recoveryOf(event, view.group.did) };
};

export const actions = recoveryActions;

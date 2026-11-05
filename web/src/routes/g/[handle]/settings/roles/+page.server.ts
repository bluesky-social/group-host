import { error } from '@sveltejs/kit';
import type { PageServerLoad } from './$types';
import { roleActions } from '#lib/server/settings.ts';

/** Its roles, to one who configures it. */
export const load: PageServerLoad = async (event) => {
	if (!(await event.parent()).configures) error(403, 'Only those who run the group change its roles.');
};

export const actions = roleActions;

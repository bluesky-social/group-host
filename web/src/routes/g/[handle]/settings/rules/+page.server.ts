import { error } from '@sveltejs/kit';
import type { PageServerLoad } from './$types';
import { ruleActions } from '#lib/server/settings.ts';

/** Its rules, to one who configures it. */
export const load: PageServerLoad = async (event) => {
	if (!(await event.parent()).configures) error(403, 'Only those who run the group change its rules.');
};

export const actions = ruleActions;

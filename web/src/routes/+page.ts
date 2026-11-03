import type * as listGroups from '#lib/lexicons/fyi/opensocial/listGroups.ts';
import type { PageLoad } from './$types';
import { xrpc } from '#lib/data.ts';

/**
 * The groups this host runs. A host that does not answer yet, as while
 * its methods are being brought over, is drawn as one with none.
 */
export const load: PageLoad = async (event) => {
	const answer = await xrpc<typeof listGroups.main>(event, 'fyi.opensocial.listGroups', {}).catch(() => null);
	return { groups: answer?.groups ?? [] };
};

import type * as getActorGroups from '#lib/lexicons/fyi/opensocial/getActorGroups.ts';
import type * as getActorInvites from '#lib/lexicons/fyi/opensocial/getActorInvites.ts';
import type { PageLoad } from './$types';
import { xrpc } from '#lib/data.ts';

/**
 * The groups of whoever is signed in and those inviting them; for no
 * one signed in, nothing, as the page only
 * asks them to. A host that fails to answer fails the page, rather than
 * be drawn as one with none.
 */
export const load: PageLoad = async (event) => {
	const { viewer } = await event.parent();
	if (!viewer) return { mine: null, invitations: [] };
	const mine = await xrpc<typeof getActorGroups.main>(event, 'fyi.opensocial.getActorGroups', { actor: viewer.did });
	const invited = await xrpc<typeof getActorInvites.main>(event, 'fyi.opensocial.getActorInvites', {});
	return { mine: mine.groups, invitations: invited.invites };
};

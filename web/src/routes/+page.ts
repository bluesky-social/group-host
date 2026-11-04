import type * as getActorGroups from '#lib/lexicons/fyi/opensocial/getActorGroups.ts';
import type * as getActorInvites from '#lib/lexicons/fyi/opensocial/getActorInvites.ts';
import type * as listGroups from '#lib/lexicons/fyi/opensocial/listGroups.ts';
import type { PageLoad } from './$types';
import { xrpc } from '#lib/data.ts';

/**
 * The groups of whoever is signed in and those inviting them, and every
 * group this host runs. A
 * host that does not answer yet, as while its methods are being brought
 * over, is drawn as one with none.
 */
export const load: PageLoad = async (event) => {
	const { viewer } = await event.parent();
	const all = await xrpc<typeof listGroups.main>(event, 'fyi.opensocial.listGroups', {}).catch(() => null);
	const mine = viewer
		? await xrpc<typeof getActorGroups.main>(event, 'fyi.opensocial.getActorGroups', { actor: viewer.did }).catch(() => null)
		: null;
	const invited = viewer
		? await xrpc<typeof getActorInvites.main>(event, 'fyi.opensocial.getActorInvites', {}).catch(() => null)
		: null;
	return { groups: all?.groups ?? [], mine: mine?.groups ?? null, invitations: invited?.invites ?? [] };
};

import type * as getViewer from '#lib/lexicons/fyi/opensocial/getViewer.ts';
import type { LayoutLoad } from './$types';
import { xrpc } from '#lib/data.ts';

/**
 * Who is looking, as their session cookie says: no one, from a host that
 * does not say, or fails to, as the page is no less worth drawing.
 */
export const load: LayoutLoad = async (event) => {
	const answer = await xrpc<typeof getViewer.main>(event, 'fyi.opensocial.getViewer', {}).catch(() => null);
	return { viewer: answer?.actor ?? null };
};

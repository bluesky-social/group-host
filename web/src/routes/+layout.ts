import type * as getActorInvites from '#lib/lexicons/fyi/opensocial/getActorInvites.ts';
import type * as getViewer from '#lib/lexicons/fyi/opensocial/getViewer.ts';
import type { LayoutLoad } from './$types';
import { xrpc } from '#lib/data.ts';

/**
 * Who is looking, as their session cookie says: no one, from a host that
 * does not say, or fails to, as the page is no less worth drawing. With
 * them, the invites waiting for them, which every page's header marks:
 * none, from a host that fails to say. And where someone with no account
 * makes one, as the host says, Bluesky's when it does not.
 */
export const load: LayoutLoad = async (event) => {
  const answer = await xrpc<typeof getViewer.main>(event, 'fyi.opensocial.getViewer', {}).catch(
    () => null
  );
  const viewer = answer?.actor ?? null;
  const invited = viewer
    ? await xrpc<typeof getActorInvites.main>(event, 'fyi.opensocial.getActorInvites', {}).catch(
        () => null
      )
    : null;
  return {
    viewer,
    invitations: invited?.invites ?? [],
    signUpAt: answer?.signUpAt ?? 'https://bsky.social'
  };
};

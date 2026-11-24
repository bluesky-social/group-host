import type * as getActorInvites from '#lib/lexicons/org/atgrp/internal/getActorInvites.ts';
import type * as getViewer from '#lib/lexicons/org/atgrp/internal/getViewer.ts';
import type { LayoutLoad } from './$types';
import { xrpc } from '#lib/data.ts';

/**
 * Who is looking, as their session cookie says: no one, from a host that
 * does not say, or fails to, as the page is no less worth drawing. With
 * them, the invites waiting for them, which every page's header marks:
 * none, from a host that fails to say. And where someone with no account
 * makes one, as the host says, Bluesky's when it does not; and the other
 * accounts this browser is signed in to, to switch to.
 */
export const load: LayoutLoad = async (event) => {
  const answer = await xrpc<typeof getViewer.main>(event, 'org.atgrp.internal.getViewer', {}).catch(
    () => null
  );
  const viewer = answer?.actor ?? null;
  const invited = viewer
    ? await xrpc<typeof getActorInvites.main>(
        event,
        'org.atgrp.internal.getActorInvites',
        {}
      ).catch(() => null)
    : null;
  return {
    viewer,
    invitations: invited?.invites ?? [],
    accounts: viewer ? (answer?.accounts ?? []) : [],
    signUpAt: answer?.signUpAt ?? 'https://bsky.social'
  };
};

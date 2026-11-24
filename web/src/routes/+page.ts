import type * as getActorGroups from '#lib/lexicons/org/atgrp/internal/getActorGroups.ts';
import type { PageLoad } from './$types';
import { xrpc } from '#lib/data.ts';

/**
 * The groups of whoever is signed in, beside those inviting them, which
 * the layout read; for no one signed in, nothing, as the page only asks
 * them to. A host that fails to answer fails the page, rather than be
 * drawn as one with none.
 */
export const load: PageLoad = async (event) => {
  const { viewer } = await event.parent();
  if (!viewer) return { mine: null };
  const mine = await xrpc<typeof getActorGroups.main>(event, 'org.atgrp.internal.getActorGroups', {
    actor: viewer.did
  });
  return { mine: mine.groups };
};

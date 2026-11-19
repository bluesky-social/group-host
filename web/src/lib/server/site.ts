// A group's website, as its public repo holds it: what the group's page
// links to, and what its settings publish and delete.

import { error } from '@sveltejs/kit';

/** The record of a group's site, at `self` in its public repo. */
const SITE = 'fyi.opensocial.site';

/**
 * A site as the pages show it: where it is, how many files it has, when
 * it was published, the page every address goes to if any, the host's
 * methods its pages read, and what they write for whoever signs in.
 */
export type Site = {
  url: string;
  files: number;
  publishedAt: string;
  fallback?: string;
  reads: string[];
  writes: { space: string; collection: string }[];
};

type Load = { fetch: typeof fetch; url: URL };

/**
 * The group's site, or null while it has none: served at `address`, over
 * https wherever the console is. Anyone may read it, as the record is
 * public.
 */
export async function siteOf(event: Load, group: string, address: string): Promise<Site | null> {
  const query = new URLSearchParams({ repo: group, collection: SITE, rkey: 'self' });
  const res = await event.fetch(`/xrpc/com.atproto.repo.getRecord?${query}`);
  const answer = await res.json().catch(() => null);
  if (!res.ok) {
    if (answer?.error === 'RecordNotFound') return null;
    error(res.status, answer?.message ?? res.statusText);
  }
  return {
    url: `${event.url.protocol}//${address}/`,
    files: answer.value.files?.length ?? 0,
    publishedAt: answer.value.createdAt,
    fallback: answer.value.fallback,
    reads: (answer.value.reads ?? []).map((r: { method: string }) => r.method),
    writes: answer.value.writes ?? []
  };
}

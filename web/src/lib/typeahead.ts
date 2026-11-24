// Finding an account by what has been typed of its handle, from the host
// (fyi.opensocial.internal.searchActors), which asks a typeahead service that knows
// everyone on the Atmosphere, or on a network of its own reads its own
// directory's handles. What is typed goes only to the host.

/** An account the host matched. */
export type Actor = { did: string; handle: string; displayName?: string; avatar?: string };

const SEARCH = '/xrpc/fyi.opensocial.internal.searchActors';

/** Who matches what is typed, best first: none for under two letters, or when the host cannot say. */
export async function findActors(q: string, signal?: AbortSignal, limit = 6): Promise<Actor[]> {
  const text = q.trim().replace(/^@/, '');
  if (text.length < 2) return [];
  const res = await fetch(`${SEARCH}?q=${encodeURIComponent(text)}&limit=${limit}`, {
    signal
  }).catch(() => null);
  if (!res?.ok) return [];
  const body = (await res.json().catch(() => null)) as { actors?: Actor[] } | null;
  return (body?.actors ?? []).filter((a) => a.handle && a.handle !== 'handle.invalid');
}

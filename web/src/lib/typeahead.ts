// Finding an account by what has been typed of its handle or name, from the
// typeahead service typeahead.waow.tech, which knows everyone on the
// Atmosphere. What is typed goes to it; the console's CSP lets it.

/** An account the typeahead service matched. */
export type Actor = { did: string; handle: string; displayName?: string; avatar?: string };

const SEARCH = 'https://typeahead.waow.tech/xrpc/tech.waow.typeahead.searchActors';

/** Who matches what is typed, best first: none for under two letters, or when the service cannot say. */
export async function findActors(q: string, signal?: AbortSignal, limit = 6): Promise<Actor[]> {
	const text = q.trim().replace(/^@/, '');
	if (text.length < 2) return [];
	const res = await fetch(`${SEARCH}?q=${encodeURIComponent(text)}&limit=${limit}`, { signal }).catch(() => null);
	if (!res?.ok) return [];
	const body = (await res.json().catch(() => null)) as { actors?: Actor[] } | null;
	return (body?.actors ?? []).filter((a) => a.handle && a.handle !== 'handle.invalid');
}

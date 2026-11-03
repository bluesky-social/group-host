// What a page shows, read from the group host, typed by the lexicons'
// schemas under lexicons/, which `npm run lex` makes into src/lib/lexicons.

import { error } from '@sveltejs/kit';
import type { InferInput, InferMethodOutputBody, LexErrorData, Query } from '@atproto/lex';

type Load = { fetch: typeof fetch };

/** What the query `Q` answers. */
export type Answer<Q extends Query> = InferMethodOutputBody<Q, never>;

/** What the query `Q` is asked with, its defaults left to the server. */
export type Asked<Q extends Query> = InferInput<Q['parameters']>;

/** A query's parameters as its address has them: an array's values each under its name. */
function searched(params: object): URLSearchParams {
	const query = new URLSearchParams();
	for (const [k, v] of Object.entries(params)) for (const one of v === undefined ? [] : [v].flat()) query.append(k, String(one));
	return query;
}

/**
 * What the XRPC query `Q`, by its NSID `nsid`, answers for this page, given
 * `params`. A failure is the page's, but for a thing not there when the
 * page can do without it (`missing`), as null.
 */
export async function xrpc<Q extends Query>(load: Load, nsid: Q['nsid'], params: Asked<Q>, missing: true): Promise<Answer<Q> | null>;
export async function xrpc<Q extends Query>(load: Load, nsid: Q['nsid'], params: Asked<Q>, missing?: false): Promise<Answer<Q>>;
export async function xrpc<Q extends Query>({ fetch }: Load, nsid: Q['nsid'], params: Asked<Q>, missing = false): Promise<Answer<Q> | null> {
	const query = searched(params);
	const res = await fetch(`/xrpc/${nsid}${query.size ? `?${query}` : ''}`);
	if (!res.ok) {
		const failure: Partial<LexErrorData> | null = await res.json().catch(() => null);
		// An XRPC method says a thing is not there by the name of its failure.
		const status = failure?.error?.endsWith('NotFound') ? 404 : res.status;
		if (status === 404 && missing) return null;
		error(status, failure?.message ?? res.statusText);
	}
	return res.json();
}

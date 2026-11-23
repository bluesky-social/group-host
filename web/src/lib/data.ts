// What a page shows, read from the group host, typed by the lexicons'
// schemas under lexicons/, which `npm run lex` makes into src/lib/lexicons.

import { error, fail } from '@sveltejs/kit';
import type { InferInput, InferMethodOutputBody, Query } from '@atproto/lex';
import { refusal, type Failure } from '#lib/errors.ts';

type Load = { fetch: typeof fetch };

/** What the query `Q` answers. */
export type Answer<Q extends Query> = InferMethodOutputBody<Q, never>;

/** What the query `Q` is asked with, its defaults left to the server. */
export type Asked<Q extends Query> = InferInput<Q['parameters']>;

/** A query's parameters as its address has them: an array's values each under its name. */
function searched(params: object): URLSearchParams {
  const query = new URLSearchParams();
  for (const [k, v] of Object.entries(params))
    for (const one of v === undefined ? [] : [v].flat()) query.append(k, String(one));
  return query;
}

/**
 * What the XRPC query `Q`, by its NSID `nsid`, answers for this page, given
 * `params`. A failure is the page's, but for a thing not there when the
 * page can do without it (`missing`), as null.
 */
export async function xrpc<Q extends Query>(
  load: Load,
  nsid: Q['nsid'],
  params: Asked<Q>,
  missing: true
): Promise<Answer<Q> | null>;
export async function xrpc<Q extends Query>(
  load: Load,
  nsid: Q['nsid'],
  params: Asked<Q>,
  missing?: false
): Promise<Answer<Q>>;
export async function xrpc<Q extends Query>(
  { fetch }: Load,
  nsid: Q['nsid'],
  params: Asked<Q>,
  missing = false
): Promise<Answer<Q> | null> {
  const query = searched(params);
  const res = await fetch(`/xrpc/${nsid}${query.size ? `?${query}` : ''}`);
  if (!res.ok) {
    const failure: Failure = await res.json().catch(() => null);
    // An XRPC method says a thing is not there by the name of its failure.
    const status = failure?.error?.endsWith('NotFound') ? 404 : res.status;
    if (status === 404 && missing) return null;
    error(status, refusal(failure, res.status));
  }
  return res.json();
}

/**
 * What the XRPC procedure `nsid` answers for `input`, asked from the
 * server (a form's action), as the person signed in. A failure is the
 * page's, in the console's words for it (`#lib/errors.ts`).
 */
export async function call<T>({ fetch }: Load, nsid: string, input: object): Promise<T> {
  const res = await fetch(`/xrpc/${nsid}`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(input)
  });
  if (!res.ok) {
    const failure: Failure = await res.json().catch(() => null);
    error(res.status, refusal(failure, res.status));
  }
  return res.json();
}

/**
 * Does the XRPC procedure `nsid` with `input`, from a form's action: if
 * the host refuses it, the form says why (`failed`), on the page it was
 * sent from, rather than the page failing.
 */
export async function act({ fetch }: Load, nsid: string, input: object) {
  const res = await fetch(`/xrpc/${nsid}`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(input)
  });
  if (res.ok) return;
  const failure: Failure = await res.json().catch(() => null);
  return fail(res.status, { failed: refusal(failure, res.status) });
}

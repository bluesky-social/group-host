// The messages between a page of a group's site, sandboxed in its frame
// (kit.js, its `os`), and the host's page around it (shell.ts). Each
// carries `os: 1`; any other message either side gets is ignored. The
// frame's messages are the site's own code, so the host's page checks
// what each says rather than trusting these types.

import type { GroupRecord } from '@opensocial/sdk';

/** Who is looking, as `os.me()` answers it. */
export type Me = { signedIn: boolean; did?: string; handle?: string };

/** A record written in the visitor's own repo, as `create` and `put` answer. */
export type Written = { uri: string; cid: string };

/** What a sync of a space answers: the writers it could not read, and every record held. */
export type Synced = { failed: { repo: string; error: string }[]; records: GroupRecord[] };

/**
 * What the site's page asks, by type: what the ask carries, and what its
 * reply's `value` is.
 */
export type Asks = {
  group: { ask: object; answer: { did: string; handle: string; console: string } };
  me: { ask: object; answer: Me };
  call: { ask: { method: string; params: Record<string, unknown> }; answer: unknown };
  act: { ask: { method: string; input: object }; answer: unknown };
  sync: { ask: { space: string }; answer: Synced };
  create: {
    ask: { space: string; collection: string; value: object; rkey?: string };
    answer: Written;
  };
  put: { ask: { space: string; collection: string; value: object; rkey: string }; answer: Written };
  delete: { ask: { space: string; collection: string; rkey: string }; answer: undefined };
  upload: { ask: { file: Blob }; answer: unknown };
};

/** An ask of the site's page, answered by a `Reply` naming its `id`. */
export type Ask = {
  [T in keyof Asks]: { os: 1; type: T; id: number } & Asks[T]['ask'];
}[keyof Asks];

/** What the site's page tells, answered by nothing. */
export type Tell = { os: 1 } & (
  | { type: 'signIn'; hint?: string }
  | { type: 'signOut' }
  | { type: 'at'; path: string; title: string }
);

/**
 * Why an ask was refused: `locked` when it is for someone else to make, as
 * `reason` says; `name`, the method's own error name when it gave one.
 */
export type Refusal = {
  error: string;
  name?: string;
  locked?: true;
  reason?: 'sign in' | 'not for you';
};

/** The host's page's answer to an ask: its value, or its refusal. */
export type Reply = { os: 1; type: 'reply'; id: number } & ({ value: unknown } | Refusal);

/** What the host's page tells unasked: who is looking now, or a space's records changing. */
export type Push = { os: 1 } & (
  | { type: 'changed'; me: Me }
  | { type: 'records'; space: string; records: GroupRecord[] }
);

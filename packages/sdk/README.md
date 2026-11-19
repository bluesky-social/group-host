# @opensocial/sdk

For an app or a group's site on a group host: the host's own methods, asked
as a member through their PDS, and a group's spaces read whole, as
lex-space reads them: every writer's repo, verified, kept and read on. It
is `@atproto/lex` and `@atproto/lex-space` with the group around them, and
needs no index of the host's.

Its `fyi.opensocial.*` types are made from `../../lexicons` by `npm run
lex` (which `check` and `test` run first) into `src/lexicons`, not kept in
git. It is TypeScript source: a bundler (Vite) takes `src/index.ts` as it
is.

```ts
import { GroupClient } from '@opensocial/sdk';

const group = await GroupClient.open(session, 'bookclub.groups.example', {
  service: 'did:web:groups.example#opensocial'
});

const room = group.space(com.example.forum.room.main, {
  skey: 'general',
  store: indexedDbStore
});
room.onChange((records) => draw(records));
await room.sync();
draw(room.records({ collections: [com.example.forum.post] }));
await room.create(com.example.forum.post, { text: 'Hello' });
```

## GroupClient

`GroupClient.open(agent, group, { service })` opens a group by its DID or
handle (a handle resolved by the member's PDS) and asks the host for it.
`agent` is the member's (an OAuth session); `service` is the host's
`did:web:…#opensocial`, which the member's PDS proxies to.

| Member                                          | What                                             |
| ----------------------------------------------- | ------------------------------------------------ |
| `did`, `handle`                                 | the group's                                      |
| `agent`                                         | the member's agent                               |
| `client`                                        | a lex `Client` asking the host                   |
| `getGroup()`                                    | `getGroup`'s answer, typed                       |
| `members({ q, limit })`                         | an async iterator over `listMembers`, every page |
| `join({ message, invite })`                     | `requestJoin`                                    |
| `leave()`                                       | `leaveGroup`                                     |
| `report({ subject, reasonType, reason, rule })` | `createReport`                                   |
| `space(schema, { skey, store, ... })`           | a `GroupSpace` of the group's                    |

`listActorGroups(client, params)` and `listActorInvites(client, params)`
answer one page of `getActorGroups` and `getActorInvites`, with a lex
`Client` whose `service` is the host's. A refusal is lex's
`XrpcResponseError`, its `error` the name the method's lexicon gives
(`GroupNotFound`, `JoinClosed`, …).

## GroupSpace

A space of the group's, as the member: lex-space's `SpaceClient`, opened
with `{ authority: group, skey }`, plus the records it holds. `skey` is the
space type's literal key, or else `self`, unless given. The options are
`SpaceClient.openSpace`'s (`didResolver`, `fetch`, …), and:

- `store`: `{ get(key), put(key, value) }`, a JSON blob by key, to keep the
  records between visits (the key names the member and the space).
- `concurrency`: how many repos to pull at once, 4 by default.

| Member                                                                            | What                                                                                                  |
| --------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------- |
| `client`                                                                          | the `SpaceClient`                                                                                     |
| `uri`                                                                             | `at://<group>/space/<type>/<skey>`                                                                    |
| `sync()`                                                                          | brings the records up to date; answers `{ failed: [{ repo, error }] }`, the writers it could not pull |
| `records({ collections })`                                                        | the records held now, by writer, collection and key; `collections` by NSID or lexicon namespace       |
| `onChange(fn)`                                                                    | `fn(records)` after each sync that changed them; answers a function that stops it                     |
| `create(ns, value, { rkey })`, `put(ns, value, { rkey })`, `delete(ns, { rkey })` | writes to the member's own repo in the space, in lex-space's order                                    |

The first `sync()` reads each writer's repo in full, as lex-space's
`pullRepo` does with no head. Each later one lists the writers that moved
since its cursor (`writers({ cursor })`), and pulls each from the head it
kept, with what it holds to look up CIDs: the changes, or the whole repo
again when the log no longer reaches the head. It drops the records of a
writer removed from the set, one whose repo its host no longer has
(`empty`), and one taken down, suspended or deactivated. Once the cursor is
older than the 72 hours the host remembers removals for, it lists the
whole set again and drops whoever is missing from it. A writer whose host
fails otherwise keeps the records and head last kept of it, is answered in
`failed`, and is pulled again at the next sync, moved or not; the rest of
the sync, cursor included, is kept. A sync whose writers cannot be listed
rejects and keeps nothing of what it read, and the next reads on from where
the last one ended. One sync runs at a time: a call while one runs gets the
same promise.

A record is

```ts
type GroupRecord = {
  uri: string; // at://<group>/space/<type>/<skey>/<repo>/<collection>/<rkey>
  repo: string; // the writer's DID
  collection: string;
  rkey: string;
  cid: string;
  value: unknown; // the record in lex's JSON form
};
```

## Checks

`npm run check` (the types) and `npm test` (vitest, lex-space's network
answered by a stub of the space) in this directory.

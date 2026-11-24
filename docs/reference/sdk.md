# The SDK

`@opensocial/sdk` is a TypeScript SDK for apps on a group host. It wraps lex-space for a group's [spaces](../design/spaces.md), plus the host's own `fyi.opensocial.*` methods, asked as a member. Its shape follows lex-space's, and it hands back lex-space's `SpaceClient` for anything it doesn't wrap. It lives in `packages/sdk`, beside the host's lexicons. Roles and membership are in [membership](../design/membership.md).

It exports `GroupClient`, `GroupSpace`, `GroupRecord`, `listActorGroups` and `listActorInvites`.

## Signing in

Every call is made as a member, with an authenticated lex `Agent` of theirs, such as their OAuth session. `GroupSpace.open` throws a `TypeError` when the agent isn't authenticated.

The app asks the member for two kinds of scope. It asks a `space:` scope for each space type it reads or writes, and `rpc:fyi.opensocial.*` for the host's methods. The host's methods go to the member's PDS, which proxies them to the host's `#opensocial` service. The app names that service in the `service` option as the host's DID with the fragment (`did:web:groups.example#opensocial`).

## GroupClient

`GroupClient.open(agent, group, { service })` opens a group by its DID or handle. The member's PDS resolves a handle first (`com.atproto.identity.resolveHandle`). Then the client asks the host for the group with `getGroup`, and fails with `GroupNotFound` when the host has no such group.

The client it answers has `did` and `handle`, the group's as the host gives them. It also has `agent`, the member's, and `client`, a lex `Client` that asks the host through the member's PDS. Each method takes the group from the client, and an optional last argument of lex's `ActionOptions` (such as `signal`).

| Method | Calls | Answers | Fails with |
| --- | --- | --- | --- |
| `getGroup()` | `getGroup` | `group`, `roles`, `spaces`, `joinRequests`, `bindings` | `GroupNotFound` |
| `members({ q, limit })` | `listMembers` | an async iterator over every member, page by page | `GroupNotFound` |
| `join({ message, invite })` | `requestJoin` | `status` (`pending` or `admitted`), with `request` when pending and `membership` when admitted | `GroupNotFound`, `InvalidInvite`, `JoinClosed`, `AlreadyMember` |
| `leave()` | `leaveGroup` | nothing | `NotAMember`, `LastConfigurer` |
| `report({ subject, cid, reasonType, reason, rule })` | `createReport` | `id`, `createdAt` | `GroupNotFound`, `InvalidSubject` |
| `space(schema, options)` | nothing until used | a `GroupSpace` | |

`members` takes `q`, part of a handle or DID in any case, and `limit`, the page size (50 by default, at most 100). It asks for pages until the host answers no cursor, or the cursor it was just given.

`join` asks to join. An invite the group sent the member admits them at once, whether `invite` names it or not. If `invite` names something that isn't such an invite, the host refuses with `InvalidInvite`. `message` is up to 2,000 graphemes.

`report` reports an account or a record to the group's moderators. `subject` is an account's DID or a record's URI, and `cid`, if given, the version of the record. `reasonType` is `spam`, `violation`, `misleading`, `nsfw`, `rude`, `other`, or `appeal` for the subject's own appeal. `rule` is the URI of a rule in the group's meta space.

A refusal is lex's `XrpcResponseError`, and its `error` is the name from the method's lexicon.

## GroupSpace

A `GroupSpace` is one of the group's spaces, read whole as the member. It holds every writer's repo, verified by lex-space, and keeps reading on from there.

`group.space(schema, { skey, ...options })` opens one from a `GroupClient`. `GroupSpace.open(schema, { authority, skey }, { agent, ...options })` opens one without a client, by the group's DID. `skey` defaults to the space type's literal key, or else to `self`. Neither makes a request until the space is used.

The options are lex-space's `SpaceClient.openSpace` options (`credentialProvider`, `fetch`, and the options for building a credential provider), plus these:

- `store`: `{ get(key), put(key, value) }`, where the space is kept between visits, such as IndexedDB.
- `concurrency`: how many repos to pull at once, 4 by default.

`uri` is the space's URI, `at://<group>/space/<type>/<skey>`, and `agent` is the member's.

### Reading

`sync()` brings the records up to date. Until it has run, `records()` answers an empty list.

The first sync starts from what the store holds of the space. With nothing held, it reads each writer's repo in full. Each later sync lists the writers that moved since its cursor (lex-space's `writers({ cursor })`). It pulls each one from the head it kept, using the records it holds to look up CIDs. It gets the changes, or the whole repo again when the repo's log doesn't reach back to the kept head. If a writer's host has an older copy than the one kept, the sync keeps what it had.

A sync drops the records of a writer removed from the writer set. It also drops a writer whose repo its host doesn't have anymore, and one that's taken down, suspended or deactivated. The space host remembers removals for 72 hours. So when the writers were last listed more than 71 hours ago (an hour less, for clocks that disagree), the sync lists the whole writer set again and drops whoever is missing from it.

`sync()` answers `{ failed }`, with a `{ repo, error }` for each writer whose pull failed for any other reason. That writer keeps the records and head last kept of it, and the next sync pulls it again, whether it moved or not. The sync keeps the rest of what it did, cursor included.

When the writers can't be listed, `sync()` rejects with an `XrpcResponseError` and nothing kept changes. One example is `SpaceNotFound` when the member isn't let in. The next sync reads on from where the last one ended. One sync runs at a time, so a call while one runs gets the same promise.

`records({ collections })` answers the records held now, sorted by URI. `collections` keeps only those collections, each an NSID string or a lexicon namespace with a `$type`.

`onChange(fn)` calls `fn` with every record held, after each sync that changed them. It answers a function that stops it.

### The store

The store keeps one JSON string per space and member, under the key `<member DID> <space URI>`. It holds the cursor, when the writers were last listed, and the writers to pull again. For each writer it holds the head (`rev` and state) and the records (`cid` and `value` by collection and key). A sync writes it only when something it holds changed. If a string isn't JSON, or isn't of the store's current version, the SDK ignores it and reads the space again in full.

### Writing

`create(ns, value, { rkey })`, `put(ns, value, { rkey })` and `delete(ns, { rkey })` write to the member's own repo in the space. They go through lex-space's `SpaceClient` and take its order of arguments. `rkey` is optional for `create`. `value` is the record without its `$type`. A write doesn't change the records held, so the next `sync()` reads it back.

`client` is the space's lex-space `SpaceClient`, as the member. Anything else lex-space does with a space goes through it (single records with `get`, `listRecords`, blobs, `applyWrites`).

## GroupRecord

Each record `records()` answers is a `GroupRecord`, in JSON:

| Field | What |
| --- | --- |
| `uri` | `at://<group>/space/<type>/<skey>/<repo>/<collection>/<rkey>` |
| `repo` | the writer's DID |
| `collection` | the record's collection NSID |
| `rkey` | the record key |
| `cid` | the record's CID, as a string |
| `value` | the record in lex's JSON form |

## The member's groups and invites

`listActorGroups(client, { actor, limit, cursor })` answers one page of `getActorGroups`, as `groups` and `cursor`. For the member, it lists all their groups. For another account, it lists only the groups the member is in too. `limit` is 50 by default, at most 100.

`listActorInvites(client, { group })` answers `getActorInvites`, the member's invites still waiting, newest first. With `group`, it answers only that group's.

Both take a lex `Client` whose `service` is the host's. That's a `GroupClient`'s `client`, or `new Client(session, { service })` before any group is open.

## When to read every repo

The first read of a space downloads every writer's whole repo. We measured it with lex-space's `allRecords` on a loaded machine with debug builds:

| Space | Cold open | Downloaded | Client holds | 8 readers at once |
| --- | --- | --- | --- | --- |
| 30 records | 0.25 to 0.6 s | 15 KB | 0.3 MB | under 1 s each |
| 2,065 records | 0.5 to 0.8 s | 0.7 MB | 8 MB | 1.6 to 2.2 s each |
| 19,175 records | 3.3 to 3.6 s, 3.2 s of it checking records | 5.3 MB | 72 MB | 15 to 17 s each |

Working out views from the records (counts, threads, latest replies) takes 20 ms or less at every size. Reading on is cheap at any size. It's about 6 requests, 6 KB and 150 ms after new writes, and one request when nothing changed. With a `store`, a member pays for the full read once per device.

So reading every repo holds up for spaces of up to a few thousand records. Past that, an app reads from an index service that syncs the space.

## Not yet

The SDK acts only as the member. It can't act as the group (`getGroupAuth`), and it has no `GroupManager` for admitting, roles and spaces yet.

## Example

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
await room.sync();
```

# Spaces

A group's private records do not live in one place. Each member writes in
their own account, on their own PDS. Someone has to decide who may read
and write those records, give readers credentials, and find out what was
written so a page can show it. For a group on this host, the host does
all three.

## What a space is

A space is a private record store from atproto's permissioned-data alpha
(see https://atproto.com), named `at://<authority>/space/<type>/<skey>`:
the authority a DID, the type an NSID, the skey a record key. Each
account that writes in a space keeps its own repo there, on its own PDS.
The authority says who may read and write, and gives readers credentials.
This host is the repo host for its groups' repos in spaces, and the
authority for each group's own spaces.

## The spaces a group keeps

Starting a group makes two spaces. Meta,
`at://<group>/space/fyi.opensocial.meta/self`, holds the group's profile,
its labeler and an access record. Members,
`at://<group>/space/fyi.opensocial.members/self`, holds the roles, the
permissions, the memberships, and a `fyi.opensocial.space` record for
each of the group's spaces: the list apps read to find them. A group
listed on the network points at its meta space from a
`fyi.opensocial.declaration` record in its public repo.

A member holding `space.create` makes more with
`fyi.opensocial.createSpace`; one holding `space.delete` ends one with
`fyi.opensocial.deleteSpace`. Meta and members cannot be deleted. A space
for chosen people is changed only by them ([groups](groups.md)).

## Who writes where

The group's own repo in a space holds what the group writes as itself. A
member's repo in the same space holds what that member writes, on their
PDS. The host holds the group's repos and hears of the members' by
notice. Apps signed in as the group write through
`com.atproto.space.createRecord`, `putRecord`, `deleteRecord` and
`applyWrites` ([signing in](signing-in.md)).

A repo in a space is its records and a revision, with no signed tree. A
set hash stands in for the tree: each record adds an element built from
its `collection/rkey/cid`, and a digest of the sum names the repo's
state. A write takes out each changed record's old element and adds its
new one, so the hash moves without being recomputed. A commit is the
repo, the space, the revision and that digest.

The group also writes in spaces it is not the authority for. An invite
goes in the group's repo in the invitee's inbox,
`at://<invitee>/space/fyi.opensocial.inbox/self`. The inbox's authority
asks this host who may use it, with
`com.atproto.managedspace.checkUserAccess`: its owner may read, and a
group here may write ([membership](membership.md)).

Details:

- The set hash is an LtHash: BLAKE3 expands each element to 1024 u16
  lanes added mod 2^16; the digest is SHA-256 of the 2048-byte state.
- Records are checked as in the public repo: DRISL data, `$type` equal to
  the collection, only blobs the group uploaded.
- Writes to a repo go one at a time, each with a new TID revision; one
  whose head moved under it fails rather than overwrite.
- A blob belongs to the group and goes once no record in any of its repos
  references it. An upload no record takes within a day is swept.

## Deciding who reads and writes

Members and roles change, so a space's rule cannot be a fixed list. Each
space holds an access record, `fyi.opensocial.access/self`, in the
group's own repo there, and the host asks it against the user's roles on
every request. A user may read if `readableBy` names `"public"` or a role
they hold. Any member may write in their own name, unless `writableBy`
names who may. In a space of chosen people (`people`), those people read
and write while they are members, and reviewers read.

The group itself may do anything in its spaces. Where the access record
does not allow a user, a grant may; grants come only from moving a group
in ([moving](moving.md)). While a group is switched off, no one reads or
writes in its spaces, the group included.

## Credentials

A reader proves to a repo host that the authority lets them read with a
space credential. A user asks for one with
`com.atproto.space.getSpaceCredential`, sending a delegation token their
own key signs. The token lives 60 seconds and is spent on first use, so
one seen in transit cannot buy a second credential. A user the space does
not let read gets `SpaceNotFound`, the same as for a space not here.

A credential is a JWT the group signs for one space, bound to a P-256 key
the caller holds, and lasts 10 minutes. The host records each it issues,
so it can take them back. Putting a member out revokes their unexpired
credentials to all the group's spaces: at once for repos here, and by
`com.atproto.space.notifyCredentialRevoked` to each other writer's PDS,
best effort.

## Hearing of writes

A member's write lands on the member's PDS, so the authority knows of it
only if that PDS says so. In turn, after each write to a group's repo in
any space, this host tells the space's authority with
`com.atproto.space.notifyWrite`, signed by the group. The write stands
whether the authority heard or not. Word owed waits in an outbox, which
keeps only the newest write per repo and space, and is retried for a day.
An authority that is a group here is told at once, inside the host.

As an authority, the host takes `notifyWrite` only from the writer's own
account, for a repo the space lets write. It keeps a writer set: each
repo's latest revision and digest. `com.atproto.space.listRepos` answers
it. Other services follow a space by `registerNotify`, for a day, and get
each kept write forwarded; one that misses a notice learns of the write
with the next, or on its own next read.

## The index

A page wants a space's records in one request, not one per member's PDS,
so the host indexes its groups' spaces. A writer whose digest in the
writer set differs from the one the index reached is read again: from the
store if here, else from their PDS with a credential the group issues
itself. Where the index has read the repo before, it reads on with
`listRepoOps` from that revision, moving the set hash by each op and
checking it against the last page's commit. Otherwise, or if that fails,
it reads the repo whole with `getRepo`. Every record must match its CID.

A writer the space no longer lets write is dropped from the index, and
putting a member out drops them from every space of the group at once.
The index reads a space when its writer set moves, and every minute for
anything missed.

## Reading members' repos elsewhere

The host finds a member's PDS and keys in their DID document: a
`did:plc` one from the PLC directory, a `did:web` one from
`/.well-known/did.json` at its host. A document must name the DID asked
for. Documents are kept five minutes; a signature the kept key turns away
is checked again against a fresh one, so a key just changed still works.

`fyi.opensocial.getSpaceBlob` serves a file from whoever wrote it, with
the group's credential. It fetches only from a repo in the space's writer
set, so a caller cannot point the host at a server of their choosing, and
the bytes must hash to the CID asked for.

Details:

- `DID_WEB_SCHEME` picks `https` or `http` ([config](../reference/config.md)).
  A `did:web` with a path is refused, and redirects are not followed.
- A document is read up to 256 KiB; a file up to 5 MiB.

## Reading a space through the host

The `fyi.opensocial.*` methods answer from the index.
`fyi.opensocial.listSpaceRecords` brings the index up to date, waiting at
most 5 seconds, then answers newest first; its `createdAt` is when the
index read a record. It and `getSpaceBlob` ask the access record on every
call, so the index never decides who sees what. The caller is known by
service auth or the console's session; one with neither reads only what
is readable by `"public"`. A space's images are also served at
`/img/{did}/{cid}` to whoever the space lets read.

The `com.atproto.space.*` methods read one repo directly: `getRecord`,
`listRecords`, `getRepo`, `getLatestCommit`, `listRepoOps`, `getBlob`,
`listBlobs`. Each needs a credential from the space's authority, on a
request signed with the key it is bound to. A reader may send a key
share; the host agrees a secret against the group's `#atproto_kem` key
and returns a MAC over the commit or record. The MAC proves to that
reader alone that this host answered.

Not yet built: an app signed in as the group reading its own repos.
Parameters, limits and headers are in [the API reference](../reference/api.md).

## Ending a space or a group

Deleting a space removes its entry in members and tells its followers
with `com.atproto.space.notifySpaceDeleted`. The host forgets the space's
index, writer set, grants, issued credentials, and the group's own repo
in it. Members' repos on their own PDSes are theirs, and stay. Deleting a
group tells the followers of each of its spaces the same way, best effort.

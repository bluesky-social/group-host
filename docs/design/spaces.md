# Spaces

A group's private records don't live in one place. Each member writes in
their own account, on their own PDS. Someone has to decide who may read
and write those records, give readers credentials, and find out what was
written so a page can show it. For a group on this host, the host does
all three.

## What a space is

A space is a private record store from atproto's permissioned-data alpha
(see https://atproto.com). It's named `at://<authority>/space/<type>/<skey>`,
where the authority is a DID, the type an NSID and the skey a record key.
Each account that writes in a space keeps its own repo there, on its own
PDS. The authority says who may read and write, and gives readers
credentials. This host is the repo host for its groups' repos in spaces,
and the authority for each group's own spaces.

## The spaces a group keeps

Starting a group makes two spaces. Meta,
`at://<group>/space/org.atgrp.meta/self`, holds what the group says
about itself (`org.atgrp.about`) and its settings (`org.atgrp.config`).
Members, `at://<group>/space/org.atgrp.members/self`, holds the roles,
the memberships, the rules and the labeler. It also holds the index: a
`org.atgrp.space` record, `{space, createdAt}`, for each of the
group's other listed spaces, which apps read to find them. Meta and
members are never in it. A group listed on the network says so with an
empty `org.atgrp.declaration` record in its public repo.

A group makes more spaces, and changes and ends them, through atproto's
own space methods ([below](#making-changing-and-ending-a-space)). Meta
and members come and go with the group.

A space's type is any NSID, but a member's PDS grants an app a type only
once it can read that type's published schema.

## Who writes where

The group's own repo in a space holds what the group writes as itself. A
member's repo in the same space holds what that member writes, on their
PDS. The host holds the group's repos and hears of the members' repos by
notice. Apps signed in as the group write through
`com.atproto.space.createRecord`, `putRecord`, `deleteRecord` and
`applyWrites` ([signing in](signing-in.md)).

A repo in a space is its records and a revision, with no signed tree. A
set hash stands in for the tree. Each record adds an element built from
its `collection/rkey/cid`, and a digest of the sum names the repo's
state. A write takes out each changed record's old element and adds its
new one, so the hash moves without being recomputed. A commit is the
repo, the space, the revision and that digest.

The group also writes in spaces it isn't the authority for. An invite
goes in the group's repo in the invitee's inbox,
`at://<invitee>/space/org.atgrp.inbox/self`. The invitee's app
makes it a `com.atproto.memberspace.policy` space with `publicWrite` set
and no members, so any group may write there and only its owner reads
it. The host tells the invitee's PDS of each write
([membership](membership.md#invites)).

Details:

- The set hash is an LtHash. BLAKE3 expands each element to 1024 u16
  lanes added mod 2^16, and the digest is SHA-256 of the 2048-byte state.
- Records are checked as in the public repo: DRISL data, `$type` equal to
  the collection, only blobs the group uploaded.
- Writes to a repo go one at a time, each with a new TID revision. A
  write whose head moved under it fails instead of overwriting.
- A blob belongs to the group and goes once no record in any of its repos
  references it. An upload no record takes within a day is swept.
- A blob is read where a record holding it is. `com.atproto.sync.getBlob`
  and `listBlobs` answer only blobs the public repo holds. A blob held
  only in a space is read through the space, by those who may read it.

## Deciding who reads and writes

Members and roles change, so a space's rule can't be a fixed list of
people. Each space is kept under a policy, `org.atgrp.policy`, that
speaks of roles. The host keeps it, and checks it against the user's
roles on every request. It holds:

- `grants`, at most one a role. Each says whether the role's members
  read the space (`read`), whether they write there in their own name
  (`write`), and which permissions they hold there acting as the group
  (`permissions`), such as `space:<type>?collection=<nsid>`. A
  permission naming another space grants nothing.
- `publicRead`: anyone at all may read the space, signed in or not.
- `unlisted`: the space is left out of the group's index. Meta and
  members are never in it, so on their policies it changes nothing.

A user reads the space if it's `publicRead`, or if a role they hold is
granted `read`. Only members whose roles are granted `write` are in the
space's writer set, so only their records reach readers. Someone who
loses `write`, by a change of roles or policy, by leaving or by being
put out, leaves the writer set at once.

The group itself may do anything in its spaces. Where the policy doesn't
allow a user, a grant of their own may. Such grants come only with a
space moved here ([moving](moving.md)). While a group is switched off,
no one reads or writes in its spaces, the group included.

The host publishes each policy in its space, as the group's
`com.atproto.space.policy/self` record there (`{policy}`), so anyone who
reads the space can read its rules. It keeps the index in step with
`unlisted`. Both are the host's alone: a write of either as a record is
refused, and they change only when the policy does.

## Making, changing and ending a space

A space is made, changed, ended and read by atproto's own methods,
`com.atproto.space.createSpace`, `updateSpace`, `deleteSpace` and
`getSpace`, called as the group. That's an app signed in as the group,
or a group token from `org.atgrp.internal.getGroupAuth`
([signing in](signing-in.md)). An app password can't call them, since
its session never reaches spaces. Each call is checked twice:

- The session's scope must hold `space:<type>?manage=create`,
  `manage=update` or `manage=delete` for the space, or it fails
  `ScopeMissing`.
- The member's roles must allow it at that moment, or it fails
  `NotAuthorized`. Making a space takes a role granting `space.create`.
  Changing one takes a role granting `space.configure`, or a
  `manage=update` scope that the space's own policy grants one of their
  roles. Ending one takes `space.delete`, or a `manage=delete` scope in
  its policy.

`createSpace` takes a type, a key (or makes a new one) and a policy. Any
policy but `org.atgrp.policy` fails `UnsupportedPolicy`. The host
also refuses a grant of a role the group doesn't have, two grants of one
role, a permission that's no space permission, and the meta and members types. A
space is made whole or not at all: if its policy can't be published,
the host ends it again. Unless `unlisted`, it gets an index entry.

`updateSpace` replaces the policy whole, publishes it, and lists or
unlists the space to match. It grants no more than its caller holds. It
changes only the grants of roles they may assign, adds only permissions their
roles hold, and grants `read` or `write` (or turns on `publicRead`) only
if they have it there. Anything else fails `NotAuthorized`. A new
group's admins hold every permission in its spaces (`space:*?collection=*`),
so they may give any. On the members space, `member` keeps `read` and
`write`, and `publicRead` stays off: every member holds `member`, and
nobody else reads the roster. Any other policy there fails
`MembersSpaceFixed`. Then each writer the new policy no longer lets
write leaves the writer set. Changing `publicRead` on meta writes or
removes the group's declaration.

`deleteSpace` takes the space out of the index, then ends it
([below](#ending-a-space-or-a-group)). Meta and members can't be deleted
(`NotAuthorized`). `getSpace` answers a space and its policy to a member
who reads it or may change it.

A group token for a space carries the `manage=` operations its member
may use there. One may be asked for a space not made yet, to make it
with `manage=create`.

## Credentials

A reader proves to a repo host that the authority lets them read by
showing a space credential. A service syncing a space asks for one with
`com.atproto.space.authorizeSpaceSync`. It names the P-256 key the
credential will be bound to and sends a delegation token the user's own
key signs. The token lives 60 seconds and is spent on first use, so
someone who sees it in transit can't buy a second credential. If the
space doesn't let the user read, the answer is `SpaceNotFound`, the same
as for a space that isn't here. A space deleted since gets
`SpaceDeleted`, and a space of a group that's switched off gets
`RepoDeactivated`. The host doesn't read a client attestation if one is
sent, since no space here lets in only some apps.

A credential is a JWT the group signs for one space. It's bound to that
key and lasts 10 minutes. The answer also gives the space's revision,
which is the point in its writer set the credential was issued at. The
host records each credential it issues, so it can take them back.
Putting a member out revokes their unexpired credentials to all the
group's spaces. That happens at once for repos here, and by
`com.atproto.space.notifyCredentialsRevoked` to each other writer's PDS,
best effort.

## Hearing of writes

A member's write lands on the member's PDS, so the authority knows of it
only if that PDS says so. In turn, after each write to a group's repo in
any space, this host tells the space's authority with
`com.atproto.space.notifyRepoUpdate`, signed by the group. The write
stands whether the authority heard or not. A notice the host still owes
waits in an outbox, which keeps only the newest write per repo and space
and retries it for a day. When the authority is a group here, the host
tells it at once, internally.

As an authority, the host takes `notifyRepoUpdate` only from the
writer's own account, for a repo the space lets write. It keeps a writer
set, which is each repo's latest revision and digest. A space revision
numbers each step of the set. A member who leaves or is put out comes
out of the set at a revision of its own, and a newer write puts a writer
back. `com.atproto.space.listWriters` answers the set in revision order,
the removals included, so a syncer reads on from the last revision it
saw.

A syncing service asks to hear of changes by sending `notify` with
`authorizeSpaceSync`. That's an endpoint and a token of its own. The
host first asks the endpoint to confirm with
`com.atproto.space.confirmRegistration`, echoing a random challenge. If
it doesn't, the host refuses with `RegistrationNotConfirmed`. The
registration lasts as long as the credential. Each authorization renews
it, and a refused one withdraws it.

Each change to the writer set goes to every live registration as
`com.atproto.space.notifySpaceUpdate`, once, best effort. A syncer that
misses one learns of it from `listWriters`. The host tells a lapsed
registration so once, and tells every registration when a space is
deleted. It keeps those two notices and retries them for three days.

Details:

- An endpoint is https, or http while the host itself is. The host
  reaches it only at public addresses, without following redirects, and
  gives it 10 seconds to answer. A token is 512 bytes at most.
- The host refuses a revision more than 5 minutes ahead of its clock
  (`FutureRev`).
- A change goes to 50 registrations at a time. The lapse and deletion
  notices are retried after a minute, 10 minutes, an hour, 6 hours, then
  every 12 hours. A deletion replaces a lapse notice still owed. A lapse
  notice is dropped once the syncer renews, and a registration whose
  lapse notice is refused or never lands is forgotten.

## The index

A page wants a space's records in one request instead of one per
member's PDS, so the host indexes its groups' spaces. When a writer's
digest in the writer set differs from the one the index reached, the
index reads that writer again. It reads from the store if the repo is
here, or else from their PDS with a credential the group issues itself.
Where the index has read the repo before, it reads on with `listRepoOps`
from that revision. It moves the set hash by each op and checks it
against the last page's commit. Otherwise, or if that fails, it reads
the repo whole with `getRepo`. Every record must match its CID.

When the space no longer lets a writer write, the index drops them.
Putting a member out drops them from every space of the group at once.
The index reads a space when its writer set moves, and every minute for
anything missed.

## Reading members' repos elsewhere

The host finds a member's PDS and keys in their DID document. It gets a
`did:plc` one from the PLC directory, and a `did:web` one from
`/.well-known/did.json` at its host. A document must name the DID asked
for. The host keeps documents five minutes. When the kept key turns a
signature away, the host checks it again against a fresh document, so a
key that just changed still works.

`org.atgrp.internal.getSpaceBlob` serves a file from the member who wrote
it, with the group's credential. It fetches only from a repo in the
space's writer set, so a caller can't point the host at a server of
their choosing. The bytes must hash to the CID asked for.

Details:

- `DID_WEB_SCHEME` picks `https` or `http` ([config](../reference/config.md)).
  A `did:web` with a path is refused, and redirects aren't followed.
- A document is read up to 256 KiB, and a file up to 5 MiB.

## Reading a space through the host

The `org.atgrp.*` methods answer from the index.
`org.atgrp.internal.listSpaceRecords` brings the index up to date, waiting
at most 5 seconds, then answers newest first. It orders by when the
index first saw each record, its `createdAt`, which reading a repo again
doesn't move. It and `getSpaceBlob` check the policy on every call, so
the index never decides who sees what. The caller is known by service
auth or the console's session. A caller with neither reads nothing
through these methods, even in a `publicRead` space, and is told
`AuthMissing`. Such a space is read without signing in through
`com.atproto.space.*`, with a credential `authorizeSpaceSync` gives
anyone for it.
The host also serves a space's images at `/img/{did}/{cid}` to anyone
signed in whom the space lets read, but none the group has taken down.

The `com.atproto.space.*` methods read one repo directly. They're
`getRecord`, `listRecords`, `getRepo`, `getLatestCommit`, `listRepoOps`,
`getBlob` and `listBlobs`. Each needs a credential from the space's
authority, on a request signed with the key it's bound to. `getRecord`,
`getRepo`, `getLatestCommit` and `listRepoOps` answer only a reader who
asks for a MAC in `Accept-Signature`, with a key share. The host then
agrees a secret against the group's `#atproto_kem` key and signs the
commit or record with it as an HTTP message signature. The MAC proves to
that reader alone that this host answered. The host refuses a request
without one, or with a share that isn't a P-256 key, with
`InvalidSpaceMac`.

An app signed in as the group reads a space the same way, as an app
signed in to a person's PDS does. It asks
`com.atproto.space.getDelegationToken` for a delegation token the group
signs, and buys a credential with it from `authorizeSpaceSync`. The
session's scope must read the space, and its member must read it at that
moment, so a session never reads more than the member who approved it.
The credential is the group's, so it reads everything the space holds,
but only while the grant lasts and its member reads the space: when
either ends, the host revokes it and withdraws the registration it
bought ([signing in](signing-in.md#checking-each-write)).
Parameters, limits and headers are in
[the API reference](../reference/api.md).

## Ending a space or a group

Deleting a space removes its entry in the index, if it has one, and
tells every service registered to it with `spaceDeleted` by
`notifySpaceUpdate`, retried as
above. The host forgets the space's index, writer set, registrations,
grants and issued credentials, and the group's own repo in it. It keeps
only the fact that the space was deleted, so `authorizeSpaceSync`
answers `SpaceDeleted` until someone makes a space at the same address
again. Members' repos on their own PDSes are theirs, and they stay.
Deleting a group tells the services registered to each of its spaces
once, best effort, and keeps nothing.

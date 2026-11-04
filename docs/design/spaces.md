# Spaces

A space is the space alpha's permissioned place for records: a space of
an authority (`at://<authority>/space/<type>/<skey>`), in which each
account that writes there keeps a repo of its own at its own PDS. The
authority says who may read and write; each repo host keeps its
accounts' repos and serves them to those the authority lets read.

This host is both, for its groups: the repo host for a group's repos in
spaces wherever their authority is, and the authority for the group's
own spaces.

## A repo in a space

A group's repo in a space is its records there and a revision, as its
public repo is, but with no signed tree. Its records are summed into a
set hash (an LtHash: each record's `collection/rkey/cid` expanded to
1024 lanes and added), and a commit is the repo, the space, the
revision and the digest of that hash. A write updates the hash by
taking out each record's old element and adding its new one, so it
never reads the repo whole.

Each write is also kept as ops (the path, the CID after and before, by
revision and place), so a syncer can catch up from a revision it has
rather than read the repo again.

A record in a space is checked as a record in the public repo is: by the
data model, of its collection's `$type`, and referencing only blobs
uploaded to the group. A blob is the group's, whichever repo references
it, and goes once no record in any of them does.

## Telling the authority

After each write the host sends the space's authority, at its PDS,
`com.atproto.space.notifyWrite`: the repo's revision and digest, with
service auth signed by the group, for `<authority>#atproto_pds`. The
authority keeps it, and tells the services syncing the space.

The write stands whether the authority heard or not. The host owes the
authority word of it, kept in an outbox and tried at once: a refusal
that may pass (no answer, a 5xx, a 429) is tried again ten seconds
later, then twice as long each time, up to an hour, until it lands or a
day has passed; a refusal for good is given up. Word of a newer write
to the same space takes the place of word still owed. This is pds.js's
`outbox.js`. A space whose authority is a group here hears of the
write at once, in the host. An authority may be a `did:plc` or a
`did:web`.

## Being the authority

A group here is the authority for its own spaces. Who may do what in
one is the host's to say, from its grants: the group itself anything;
another account what it is granted, reading, writing or both; and
anyone, reading a space made open to all. Groups and roles, as they
arrive, fill the grants in, as the prototype decided each space's
access from the group's own records.

- `getSpaceCredential` issues a credential, bound to a P-256 key of the
  caller's, to the user a delegation token names: one their PDS signed
  for `<group>#atproto_pds` and this space, spent once. A user the space
  does not let read is told there is no such space. Without a token, a
  credential is issued only for a space open to all.
- `notifyWrite` is how a repo host tells the authority of a write: from
  the writer's own account, for this authority, of a revision not
  ahead of the clock, by one the space lets write. The writer set keeps
  each repo's latest revision and digest, and the space's revision when
  it heard; `listRepos` answers it, a page at a time, to a credential's
  holder.
- `registerNotify` has the authority tell a service of each write for a
  day, at the endpoint the service's own DID document names; a write
  is forwarded to each, signed by the group, best effort. A syncer that
  misses one learns of it with the next.

Not yet: deleting a space and telling its syncers, and revoking the
credentials of one who loses their grant, which come with groups and
membership.

## The index

The host keeps a copy of what is written in its groups' spaces, so it can
answer for a space in one request where a reader would need a credential
and a listing per writer. As each space's authority it knows the writer
set: when a writer's digest is not the one the index came to, the index
reads their repo, as knotless's syncer reads a space.

- A writer here is read from the store.
- A writer elsewhere is read from their PDS, with a credential the group
  issues itself, bound to a key made for the read, and a secret agreed
  against the writer's `#atproto_kem` key for each request. Read before,
  their repo is read on from its oplog (`listRepoOps`), the set hash
  moved by each write and checked against the commit their host vouches
  for, each record checked against its write's CID. Not read before, or
  if that does not check out, it is read whole (`getRepo`, a STAR
  archive) and checked the same way. A record that is not a DRISL map is
  hashed, as the commit covers it, and left out.

The index reads a space as its writer set moves, in the background, and
before it answers for it. `fyi.opensocial.listSpaceRecords` lists it,
newest first, by collection, by a field's value, or one by URI;
`fyi.opensocial.getSpaceBlob` serves a file in it, read from whoever
wrote it (5 MB at most, and only the file asked for). Both answer only
those the space's access record lets read, asked on every call: the
index holds copies, and never decides who sees them.

## Others' identities

The host reads other accounts' DID documents (a space's authority, an
authority revoking a credential) from the PLC directory for a
`did:plc`, and from `/.well-known/did.json` at its host for a `did:web`
(`DID_WEB_SCHEME`, `https` unless it is a network of test servers). A
document must be the DID's own, its `id` that DID. Each is kept five
minutes, so the same authority's credentials are checked without a
lookup each; a signature the kept key turns away is checked again with
the document read afresh, so a key the authority moved to counts at
once. A key it moved away from counts for those minutes still.

Whose document is read is a caller's to say: the issuer of a credential
or a service token is read before it is checked. So any caller can have
the host GET `/.well-known/did.json` from a host of their choosing, as
any server resolving a `did:web` can be made to. The host reads at most
256 KiB of it, keeps at most 10,000 documents, and does not refuse
private or loopback addresses: a host whose network reaches services
that answer `/.well-known/did.json` to anyone inside it should keep its
outgoing requests from reaching them.

## Reading

A repo in a space is read only with a space credential, which the
space's authority issues to someone it lets read the space, bound to a
P-256 key of theirs:

- the credential is the authority's, for this space, not lapsed, and not
  revoked (an authority revokes with
  `com.atproto.space.notifyCredentialRevoked`, which is kept until the
  credential would have lapsed anyway);
- the request names this repo as its audience, and is signed with the
  key the credential is bound to;
- the reader sends a key share in `Atproto-Space-Want-Mac`, from which
  the host and the reader agree a secret against the group's
  `#atproto_kem` key. The answer is vouched for with a MAC under it: a
  commit's for `getRepo`, `getLatestCommit` and the last page of
  `listRepoOps`, a record's for `getRecord`.

The MAC says the group's host answered, to this reader alone; nobody
else can check it, so nothing a reader is sent can be passed on as
proof.

A group reading its own repos, through an app it signed in to, comes
with signing in.

## From the prototype

The prototype's repo-host side is `pds/api/space.ts` over
`@atproto/space` and the reference PDS's store. Here it is the same
methods, less what needs signing in (writes over XRPC, an account
reading its own repo, `listSpaces`, `clearRecords`), which arrive with
signing in, and less `notifySpaceDeleted`, which a host that syncs no
other authority's spaces has nothing to do with.

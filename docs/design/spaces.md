# Spaces

A space is the space alpha's permissioned place for records: a space of
an authority (`at://<authority>/space/<type>/<skey>`), in which each
account that writes there keeps a repo of its own at its own PDS. The
authority says who may read and write; each repo host keeps its
accounts' repos and serves them to those the authority lets read.

This host is both, for its groups. This page is the repo host's side:
a group's repos in spaces, wherever their authority is. Being the
authority for a group's own spaces comes next.

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

The write stands whether the authority heard or not: it learns of the
next write instead. The host waits on it ten seconds at most, as it
waits on any server it asks. Trying again, as the reference PDS does for a day,
is still to come. An authority is found on the PLC directory, so for
now it is a `did:plc`.

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

# Moving

A group's pieces are spread out. Its identity is a `did:plc` document in
the PLC directory. Its records and files sit on this host, its PDS. Each
of its private [spaces](spaces.md) holds the group's own repo here and
its members' repos on their own PDSes. A move, of a group to another
host or of an existing account here to become a group, must never leave
it half on one host and half on another, or its identity in no one's
hands. The host changing its own address must not lose its groups either.

## Moving a group to another host

A group moves whole and keeps its DID, so whoever follows it, and
whatever points at it, still does. Its identity moves by a PLC
operation, a signed change to its DID document. The host it leaves signs
that operation with its own rotation key, the key the directory accepts
changes from.

What moves is the group's public repo, its files (blobs), and each space
the group governs: the group's own repo there, and what the authority
keeps about it (whether anyone may read it, its writers, and its
grants). A space the group only writes in, governed by another, stays.

Members' repos in the group's spaces stay on their own PDSes. The new
host keeps the writer set, each writer's revision and digest, so it can
go on reading their repos without waiting for them to write again. Their
PDSes find the new authority by the group's DID.

The move takes four calls: `migratePrepare` at the host it leaves,
`migrateReceive` at the new host, `migrateComplete` back at the host it
leaves, and `migrateActivate` at the new host. Their parameters are in
[the API reference](../reference/api.md).

Moving a group changes its configuration, so each call needs
`group.configure` on the group ([membership](membership.md)), and
whoever holds it chooses where the group goes. The stewards' recovery
keys stay ahead of the new host's key, so a steward can still take the
group back ([recovery](recovery.md)).

### Preparing

A move starts only while the host still controls the group's identity:
the directory's last operation for the group must list this host's
rotation key, or `migratePrepare` is refused `NotController`.

`migratePrepare` takes the destination host's DID and answers three
things. A service token, signed by the group's own key, lets the group
ask that host to take it in. A snapshot names the group's handle, its
address, and its spaces. An export token lets the new host pull the
group's data through `migrateExport` for an hour; preparing again
replaces it.

### Receiving

`migrateReceive` checks the service token against the signing key the
group's DID document names now, so only the group's current host could
have asked. It refuses `AlreadyHere` if the group is already an account
here. It reaches the host it leaves only at the PDS endpoint the DID
document names, never at an address the caller gives.

The pulled data must prove itself. An error the host it leaves answers,
for its repo or any other part, stops the move as `SourceUnreadable`
with that status, rather than being read as data. The public repo must
be the group's, signed by its current key, and whole. Each file must
hash to the CID it was listed under. Each space's repo must name the group and that space,
with its records' digest equal to its commit's. A file that does not
come, or does not match, fails the move: the host it leaves is still
serving, so a missing file means something is wrong, and records must
not be left pointing at nothing.

The new host makes the group signing and KEM keys of its own, and
rebuilds its repos from the pulled records under the new key; the old
commits are not kept. Only after everything is written does it check the
caller, who must hold `group.configure` by the group's own records as
they arrived, or the move is refused `Forbidden`. A failure at any point
after the account is made forgets the account and its files.

The group is kept switched off. The answer counts what came, and gives
the credentials the PLC operation should name: this host's rotation key,
the new keys, the group's handle here, and this host as its PDS.

Details:

- The group's address here is its old address's first label under this
  host's `HANDLE_DOMAIN`. A handle on a domain of its own is kept while
  it still resolves to the group.
- If that label or handle is another account's here, the move is refused
  `HandleNotAvailable`.
- Only someone this host starts groups for (`GROUP_FOUNDERS`) may bring
  a group here, as only they may start one.
- What is pulled is read up to a limit: a file to `BLOB_UPLOAD_LIMIT`, a
  repo to 256 MiB, any other answer to 16 MiB. More stops the move.
  Bringing an account here reads to the same limits, and leaves out a
  file over its limit as it does any file that does not come.

### Handing over

`migrateComplete` signs and publishes the PLC operation. It names the
new host as the group's PDS, the new keys, and the new handle. Its
rotation keys are the stewards' first, then the new host's; the old
host's own key is removed. Other services in the document are kept.
If the directory refuses the operation, as it does a key it cannot read
or more than five rotation keys, the move is refused `InvalidOperation`
with what the directory said, and the group stays switched on here.

The host it leaves then switches the group off, ends the export token,
and logs the move in the group's activity. It holds no rotation key for
the group any more, so it can neither prepare nor complete another move
of it. The group's account and data stay there, switched off.

### Switching on

`migrateActivate` is refused `NotYet` until the DID document names the
new host's `PUBLIC_URL` as its PDS and the signing key made there; asking
again once the directory has caught up succeeds. It switches the group
on, tells the relays its handle and that it is active, tells each of its
spaces of the group's repo there (written while it was off, which no
space hears of), and queues each to be read again for the index.

So the group is whole on one host at every step. Before the hand-over,
the host it leaves controls it, and a failed receive left nothing at the
new one. After it, the new host holds it, switched off until activated.

The `opensocial migrate` command makes the four calls in order
([cli](../reference/cli.md)). Run again after it handed the group over
but could not switch it on, it does only that. The console has no page for this move.

## Bringing an account here as a group

An existing `did:plc` account on any PDS can move here and become a
group, keeping its DID. A `did:web` account cannot, since its document
lives on its own domain. The account's own server signs the PLC
operation that hands it over, once its owner confirms with a code the
server emails them. Only someone this host starts groups for
(`GROUP_FOUNDERS`, or anyone when that is empty) may bring an account
here, and never their own.

### What comes along

The account's handle comes along if it is a domain of its own: not
`handle.invalid`, not under this host's `HANDLE_DOMAIN` or a domain its
server gives out, and still resolving to the DID. A server whose
`describeServer` cannot be read might give out the handle's domain, so
then the handle is not kept. The account always
gets an address here, a label under `HANDLE_DOMAIN`, asked for or made
from its current handle, and that is its handle when its own is not
kept. `checkImport` answers what both would be, or why the move would be
refused, before anything moves.

`importAccount` makes the account here, switched off, with keys of its
own, and copies its files, then its repo. The repo must be the
account's, signed by its current key, and whole. A record not valid for
its collection's schema is left out. The rest keep their CIDs, so
whatever points at them still does. A failure during the copy forgets
everything copied.

Unlike a group move, a file that does not come or does not match is
left out with a warning, and its records still point at it. An account
on an arbitrary PDS may have lost files, and that should not keep it
from moving. The old server keeps its copy until its owner deletes it.

### Its spaces

Spaces come along only from a server whose description says it speaks
the space alpha (`spacesAlpha`); from any other, the account moves
without them. A server whose description cannot be read stops the move
before its identity moves.

For each space the account governs, its state comes first, then its
files, then the account's repo there. A repo the account keeps in
another's space comes too. A personal space stays the account's alone; a
member space's members become this host's grants. Any other policy,
public writing, or a list of allowed apps is refused
`UnsupportedPolicy`, since this host's spaces have no such rules.

While the account is switched off, its space repos tell their spaces
nothing: an authority elsewhere would check them against the key the
directory still names, the old server's. Once it is switched on, each
space it has a repo in is told of its head, and the spaces it governs are
read again for the index.

Details:

- The meta and members spaces its group will make are reserved, and
  refused `SpaceReserved`.
- A part brought again replaces what came before; starting over
  discards them all.

### Handing over its identity

`completeImport` takes the operation the account's server signed. It must
name this host as the PDS, the keys made here, this host's rotation key,
and the handle here, or it is refused `InvalidOperation`. If the
directory already has it as its last operation, it is not sent again, so
a try that failed after sending can be repeated. The host then switches
the account on, tells the relays and its spaces, and makes it a group
founded by the person who moved it, with its profile's name and
description ([groups](groups.md)).

### From the console and the command

The console's page is `/move`, linked from starting a group. The person
signs in as themselves, then to the account at its own server, asking
only for a `createAccount` token for this host, the account's identity,
and reading its own repos and files in spaces
([signing in](signing-in.md)). The host keeps those tokens, one account
at a time, until the move is finished or cancelled or a day passes, then
gives them back. "Copy it here" copies the account and its spaces and
has the code emailed; "Finish moving it" takes the code and completes
the move; "Stop, and sign out of it" discards an unfinished copy.

`opensocial import` does the same from a terminal, signing in to the
account with its password, and finishes with the emailed code
([cli](../reference/cli.md)).

## When the host changes its address

When a host's address (`PUBLIC_URL`) and `did:web` (`SERVICE_DID`)
change, its groups keep their DIDs, but each DID document still names
the old address as its PDS.

`group-host move-host --from <old address>`, run with the host stopped
and the environment naming the new address, repoints them. For every
account here, it points each service at the old address to the new one,
signs the change with the host's rotation key, publishes it, and tells
the relays. It skips and reports an account whose DID is ended, and one
whose DID no longer lists this host's key because a steward took it.
An account the directory fails on is reported with why, and the rest
are still repointed; the command then exits with an error, to be run
again once the directory answers for it.
`--dry-run` reports what it would do and writes nothing.

Run again, it has nothing left to repoint, and tells the relays again
of accounts already at the new address, for a relay that started reading
after the first run. Handles under the old domain stay as they are.
Sessions are found by their hash, not their audience, so they go on
working.

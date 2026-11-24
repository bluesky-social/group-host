# Moving

A group's pieces are spread out. Its identity is a `did:plc` document in
the PLC directory. Its records and files sit on this host, its PDS. Each
of its private [spaces](spaces.md) holds the group's own repo here and
its members' repos on their own PDSes. A group can move to another host,
or an existing account here can become a group. Either way, a move must
never leave it half on one host and half on another, or leave its
identity in no one's hands. The host changing its own address mustn't
lose its groups either.

## Moving a group to another host

A group moves whole and keeps its DID. So its followers still follow it,
and anything that points at it still does. Its identity moves by a PLC
operation, a signed change to its DID document. The host it leaves signs
that operation with its own rotation key, the key the directory accepts
changes from.

The move takes the group's public repo, its files (blobs), and each space
the group governs. For a space, that's the group's own repo there and
what the authority keeps about it, as a space meta archive: its policy,
each account's grant beside the policy (as
`com.atproto.memberspace.member/<did>`), and its writers. A space the
group only writes in stays behind, since another group governs it.

Members' repos in the group's spaces stay on their own PDSes. The new
host keeps the writer set, with each writer's revision and digest. So it
can go on reading their repos without waiting for them to write again.
Their PDSes find the new authority by the group's DID.

The move takes four calls: `migratePrepare` at the host it leaves,
`migrateReceive` at the new host, `migrateComplete` back at the host it
leaves, and `migrateActivate` at the new host. Their parameters are in
[the API reference](../reference/api.md).

Moving a group changes its configuration, so each call needs
`group.configure` on the group ([membership](membership.md)). The member
who holds it chooses where the group goes. The stewards' recovery keys
stay ahead of the new host's key, so a steward can still take the group
back ([recovery](recovery.md)).

### Preparing

A move starts only while the host still controls the group's identity.
The directory's last operation for the group must list this host's
rotation key, or `migratePrepare` fails with `NotController`.

`migratePrepare` takes the destination host's DID and answers with three
things. A service token, signed by the group's own key, lets the group
ask that host to take it in. A snapshot names the group's handle, its
address, and its spaces. An export token lets the new host pull the
group's data through `migrateExport` for an hour. Preparing again
replaces it.

### Receiving

`migrateReceive` checks the service token against the signing key the
group's DID document names right now. So only the group's current host
could have asked. It fails with `AlreadyHere` if the group is already an
account here. It reaches the host it leaves only at the PDS endpoint the
DID document names, never at an address the caller gives.

The pulled data must prove itself. If the host it leaves answers with an
error, for its repo or any other part, the move stops as
`SourceUnreadable` with that status. The error is never read as data.
The public repo must be the group's, signed by its current key, and
whole. Each file must hash to the CID it was listed under. Each space's
repo must name the group and that space, and its records' digest must
equal its commit's.

A file that doesn't come, or doesn't match, fails the move. The host it
leaves is still serving, so a missing file means something is wrong.
And records mustn't be left pointing at nothing.

The new host makes group signing and KEM keys of its own. It rebuilds
the group's repos from the pulled records under the new key and doesn't
keep the old commits. Only after everything is written does it check the
caller. The caller must hold `group.configure` by the group's own records
as they arrived, or the move fails with `Forbidden`. If anything fails
after the account is made, the host forgets the account and its files.

The group stays switched off. The answer counts what came and gives the
credentials the PLC operation should name: this host's rotation key, the
new keys, the group's handle here, and this host as its PDS.

Details:

- The group's address here is its old address's first label under this
  host's `HANDLE_DOMAIN`. A handle on a domain of its own is kept while
  it still resolves to the group.
- If that label or handle belongs to another account here, the move
  fails with `HandleNotAvailable`.
- Only someone this host starts groups for (`GROUP_FOUNDERS`) may bring
  a group here, since only they may start one.
- The host reads what it pulls up to a limit: a file to
  `BLOB_UPLOAD_LIMIT`, a repo to 256 MiB, and any other answer to 16 MiB.
  Anything bigger stops the move. Bringing an account here reads to the
  same limits, and it leaves out a file over its limit just as it leaves
  out any file that doesn't come.

### Handing over

`migrateComplete` signs and publishes the PLC operation. The operation
names the new host as the group's PDS, the new keys, and the new handle.
Its rotation keys are the stewards' first, then the new host's, and the
old host's own key is removed. Other services in the document are kept.
The directory may refuse the operation (for example, a key it can't read
or more than five rotation keys). Then the move fails with
`InvalidOperation` and what the directory said, and the group stays
switched on here.

The host it leaves then switches the group off, ends the export token,
and logs the move in the group's activity. It doesn't hold a rotation
key for the group anymore, so it can't prepare or complete another move
of it. The group's account and data stay there, switched off.

### Switching on

`migrateActivate` fails with `NotYet` until the DID document names the
new host's `PUBLIC_URL` as its PDS and the signing key made there. Once
the directory has caught up, asking again succeeds. The call switches
the group on and tells the relays its handle and that it's active. It
tells each of the group's spaces about the group's repo there, since
that repo was written while the group was off and no space heard of it.
Then it queues each space to be read again for the index.

So the group is whole on one host at every step. Before the hand-over,
the host it leaves controls it, and a failed receive leaves nothing at
the new one. After it, the new host holds it, switched off until
activated.

The `opensocial group move` command makes the four calls in order
([cli](../reference/cli.md)). If it handed the group over but couldn't
switch it on, running it again does only that. The console has no page
for this move.

## Bringing an account here as a group

An existing `did:plc` account on any PDS can move here and become a
group, keeping its DID. A `did:web` account can't, since its document
lives on its own domain. The account's own server signs the PLC
operation that hands it over, once its owner confirms with a code the
server emails them. Only someone this host starts groups for
(`GROUP_FOUNDERS`, or anyone when that's empty) may bring an account
here, and never their own.

### What comes along

The account's handle comes along if it's a domain of its own. That
means it isn't `handle.invalid`, it isn't under this host's
`HANDLE_DOMAIN` or a domain its server gives out, and it still resolves
to the DID. If the server's `describeServer` can't be read, the server
might give out the handle's domain, so the handle isn't kept. The
account always gets an address here, a label under `HANDLE_DOMAIN`,
either asked for or made from its current handle. That address is its
handle when its own isn't kept. Before anything moves, `checkImport`
answers what both would be, or why the move would be refused.

`importAccount` makes the account here, switched off, with keys of its
own. It copies the account's files, then its repo. The repo must be the
account's, signed by its current key, and whole. A record that isn't
valid for its collection's schema is left out. The rest keep their CIDs,
so anything that points at them still does. If the copy fails, the host
forgets everything it copied.

A group move fails on a missing file, but an import doesn't. A file that
doesn't come or doesn't match is left out with a warning, and its
records still point at it. An account on an arbitrary PDS may have lost
files, and that shouldn't keep it from moving. The old server keeps its
copy until its owner deletes it.

### Its spaces

Spaces come along only from a server whose description says it speaks
the space alpha (`spacesAlpha`). From any other server, the account
moves without them. If the server's description can't be read, the move
stops before the account's identity moves.

For each space the account governs, its state comes first, then its
files, then the account's repo there. A repo the account keeps in
another's space comes too. Each space's policy becomes an
`fyi.opensocial.policy` that sets `unlisted`, so it's out of the group's
index. A personal space stays the account's alone. A member space's
public reading becomes `publicRead`, and its members become grants of
their own beside the policy, each reading and writing as before. Any
other policy, public writing, or a list of allowed apps fails with
`UnsupportedPolicy`, since this host's spaces have no such rules. The
host publishes each policy in its space once the group's members space
exists.

While the account is switched off, its space repos tell their spaces
nothing. An authority elsewhere would check them against the key the
directory still names, which is the old server's. Once the account is
switched on, each space it has a repo in hears of its head, and the
spaces it governs are read again for the index.

Details:

- The meta and members spaces its group will make are reserved, and
  bringing them fails with `SpaceReserved`.
- A part brought again replaces what came before. Starting over
  discards them all.

### Handing over its identity

`completeImport` takes the operation the account's server signed. The
operation must name this host as the PDS, the keys made here, this
host's rotation key, and the handle here, or it fails with
`InvalidOperation`. If the directory already has it as its last
operation, the host doesn't send it again. So you can repeat a try that
failed after sending. The host then switches the account on, tells the
relays and its spaces, and makes it a group founded by the person who
moved it. The group takes its profile's name and description
([groups](groups.md)).

### From the console and the command

The console's page is `/move`, linked from starting a group. You sign
in as yourself, then to the account at its own server. That sign-in asks
only for a `createAccount` token for this host, the account's identity,
and reading its own repos and files in spaces
([signing in](signing-in.md)). The host keeps those tokens, one account
at a time, until the move is finished or cancelled or a day passes. Then
it gives them back. "Copy it here" copies the account and its spaces and
has the code emailed. "Finish moving it" takes the code and completes
the move. "Stop, and sign out of it" discards an unfinished copy.

`opensocial account import` does the same from a terminal. It signs in
to the account with its password and finishes with the emailed code
([cli](../reference/cli.md)).

## When the host changes its address

When a host's address (`PUBLIC_URL`) and `did:web` (`SERVICE_DID`)
change, its groups keep their DIDs. But each DID document still names
the old address as its PDS.

`group-host move-host --from <old address>` repoints them. Run it with
the host stopped and the environment naming the new address. For every
account here, it points each service at the old address to the new one,
signs the change with the host's rotation key, publishes it, and tells
the relays. It skips and reports an account whose DID is ended. It also
skips one whose DID doesn't list this host's key, because a steward
took it. If the directory fails on an account, the command reports why
and still repoints the rest. It then exits with an error, and you run it
again once the directory answers for that account. `--dry-run` reports
what it would do and writes nothing.

On a second run, it has nothing left to repoint. It tells the relays
again about accounts already at the new address, in case a relay
started reading after the first run. Handles under the old domain stay
as they are. The host finds sessions by their hash, not their audience,
so they keep working.

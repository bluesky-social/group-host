# Moving a group

A group moves from one group host to another whole, and keeps its DID:
everyone who follows it, and everything that points at it, still does
(`migration.rs`).

Its identity moves by a PLC operation the host it leaves signs with its
rotation key, and everything it holds moves with it: its public repo, its
files, and in each of its spaces its own repo and what the space's
authority keeps (whether anyone may read it, who writes there and what
they last wrote, and who may beyond its access record). Members' repos in
the group's spaces are theirs, at their own PDSes, and stay there: the
new host keeps who writes, so it reads them on without waiting for them
to write again, and their PDSes find the new authority by the group's
DID.

## The steps

1. **`migratePrepare`**, at the host it leaves, for one who configures
   the group: a service token the group signs, for the new host; the
   group's handle, address and spaces; and a token, good for an hour,
   to pull the rest with. Preparing again replaces the token.
2. **`migrateReceive`**, at the new host. It takes the group in only:
   - when the service token is the group's, by its current key in the
     directory, for this host;
   - reaching the host it leaves only at the address the group's DID
     document names, and only at a public address;
   - with its public repo signed by the group and whole, each space's
     repo what its commit's set hash says, and each file what its CID
     says;
   - and for one who configures the group by its own records, as they
     arrive.

   Anything less and nothing of it stays. It is kept switched off, with
   keys of its own here, under a handle here (the same label, under this
   host's domain; a domain of the group's own kept while it still names
   the group, as this host's resolvers say), and the answer is what the
   PLC operation should name. Its address stays the same label
   under this host's domain, kept or not as its handle.
3. **`migrateComplete`**, at the host it leaves: the operation signed and
   sent, naming the new host, its keys and its handle; its stewards'
   recovery keys kept ahead of the new host's; the group switched off
   there; and its export token ended. Once handed over, it is that host's no more to hand.
4. **`migrateActivate`**, at the new host: once the directory names this
   host and the group's new key, the group switched on, and its spaces'
   index read again.

The `opensocial` command walks through the four (step 9).

## From the prototype

- A space's state moves as this host keeps it, as JSON, not as the space
  alpha's meta archive: a move between two group hosts does not need it.
  A move to a plain PDS (the prototype's `leaving`) is not here.
- The new host makes the group's repos again from the records it pulls,
  under the group's new key, rather than keeping the old commits.
- Spaces the group writes in that another governs (an invite in a
  member's inbox) do not move: they are that space's authority's.

# An account becoming a group

An account that already exists on the network, on any server and made
with any app, moves here to become a group, and keeps its DID (`import.rs`).
This is atproto's account migration with the group made at the end. The
person moving it signs in to the console as themselves, and to the
account where it lives, and asks its server for two things: a
`createAccount` service token for this host, the account's own word
that it hands its DID over; and, once the copy is made, a PLC operation
naming this host, which its server signs when the account's owner
confirms it (an emailed code, on the reference PDS).

1. **`checkImport`**: what a move would make of it, before anything
   moves. Its handle comes along if it is a domain of the account's own,
   one neither this host nor its server gives out (as bsky.social gives
   out its own), that still names it; otherwise its handle is its
   address here, its handle's first label (or one asked for) under this
   host's domain. It says when that address, or that handle, is
   another's here.
2. **`importAccount`**, for someone this host starts groups for, never
   the account itself, and only a did:plc account: the token checked
   against the account's key in the directory, for this host or one of
   its services, and for `createAccount`; the account made here switched
   off, with keys of its own; its files and then its repo copied from
   the server its DID names, the repo checked against its key and whole.
   Its records keep their CIDs, so whatever points at them still does. A
   record this host would not keep (not of its collection's type) is
   left out, and a file that does not come, or is not what its CID says,
   is left out while the records holding it still point at it, as at a
   file deleted there. Anything less and nothing of it stays. The answer
   is what the PLC operation should name. Starting again, the one who
   started discards what the first start copied; nobody else may.
3. **`completeImport`**, for whoever started: the signed operation sent
   to the directory, once it gives the account to this host as step 2
   said (this host its server, the keys made for it here, this host's
   rotation key, and its handle); the account switched on, the relays
   told who it is; and its group made, with its profile's name and
   words, run by whoever moved it.

The old server keeps its copy until its owner deletes it there; once the
DID names this host, the network reads the account here.

## From the prototype

- `importAccountSpace`, which brings the spaces such an account governs
  and its repos in others' in the space alpha's archives, is not here
  yet: it waits for the alpha to publish its meta archive, which the
  version the prototype builds against does not have.

# Moving the host

The host itself moves to a new address, the URL it answers at and its
did:web, and its groups keep their DIDs (`move_host.rs`). With the host
stopped and its environment naming the new address (`PUBLIC_URL`,
`SERVICE_DID`), `group-host move-host --from <old address>` points every
service each account's DID document names at the old address at the new
one, signed with the host's rotation key, and tells the relays who each
account is. It skips, and says so, an account whose DID is ended and one
this host holds no rotation key to any more (a steward took it).
`--dry-run` says what it would do.

Run again, it finds nothing left to repoint, and tells the relays again
of each account at the new address: a relay that starts reading the host
after the first run hears of them so.

## From the prototype

- This host keeps no managing app on its spaces and no PDS per group, so
  only the DID documents change.
- Sessions, app passwords' among them, are found by their hash, not
  their audience, so they go on working at the new address.

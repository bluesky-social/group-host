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
   host's domain; a domain of the group's own kept), and the answer is
   what the PLC operation should name.
3. **`migrateComplete`**, at the host it leaves: the operation signed and
   sent, naming the new host, its keys and its handle; its stewards'
   recovery keys kept ahead of the new host's; and the group switched off
   there. Once handed over, it is that host's no more to hand.
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

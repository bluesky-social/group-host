# Recovery

A group's identity is its `did:plc`, and whoever holds a rotation key its
PLC document names may change it. Left alone, the only one is this host's:
the host alone could move the group, and nobody could move it without
the host. Recovery keys change that (`recovery.rs`).

A recovery key is a steward's own rotation key, which the host never
sees, put ahead of the host's in the document. PLC lets a key ranked
higher undo, for 72 hours, an operation one ranked lower signed: so
whoever holds one can undo anything this host does to the group's
identity, and take the group to another host without it.

- `addRecoveryKey` (for `group.configure`) puts a `did:key` (secp256k1 or
  P-256) after any earlier recovery keys, which keep their rank, and
  ahead of the host's, which stays last. PLC takes five rotation keys, so
  a group has four recovery keys at most.
- `removeRecoveryKey` takes one off; the host's own is not one.
- `listRecoveryKeys` lists them, the highest ranked first, with who added
  each where the host added it, and whether the host still holds a key.

The keys are the directory's to say: the host remembers only who added
which, to show. Once the host holds no key (a steward took the identity
elsewhere), it changes nothing.

Deleting a group ends its DID in the directory only if nobody but the
host could ever use it again. With a recovery key on it, the DID is its
steward's, to bring the group back elsewhere, and is left alone.

The console's settings page shows them, to add and take off.

# Recovery

A group's identity is a `did:plc`. Its PLC document lists rotation keys,
and whoever holds one may change the document: point the group at
another host, or end the DID. When the host creates a group, its own key
is the only rotation key. So the host holds every group's identity
alone. A host that fails could take every group it hosts down with it,
and a host that misbehaves could take them. Recovery keys let a group's
people hold its identity too.

## The host's key

The host has one rotation key, a secp256k1 key made on its first start
and kept in its secrets. It signs every group's first PLC operation,
which names only that key. Left so, only the host could move a group,
and nobody could move it without the host. The host publishes operations
to, and reads them from, the PLC directory at `PLC_URL` (see
[config](../reference/config.md)).

## Recovery keys

A recovery key is a rotation key that a steward holds, a steward being
someone the group trusts with its identity. The steward makes the key
pair themselves and gives the host only the public half, as a `did:key`.
The host never sees the private half.

PLC ranks the rotation keys in a document. Within 72 hours, a key may
undo an operation that a lower-ranked key signed. The host places
recovery keys ahead of its own, which is always last. So a steward can
undo, within 72 hours, anything the host does to the group's identity,
and can move the group to another host without the host's help (see
[moving](moving.md)).

The host does not offer to sign anything with a recovery key, since it
never holds one. A steward uses theirs with their own PLC tooling,
against the PLC directory.

Holding a key is apart from holding a role. The group's roles decide who
may add or remove keys on the host, not who holds which key. If an admin
takes a steward's key off, the host signs that change with its own key,
which ranks below the steward's, so the steward can undo the removal
within 72 hours.

When a group's identity has left the host's control, the host stops
changing it. Once the document no longer names the host's key, adding
and removing keys are refused with `NotController`, whatever the key
asked about. The same holds once the DID has been ended.

## Adding, listing and removing keys

Adding, listing and removing keys all need `group.configure`, the action
that lets a member change the group's settings (see
[membership](membership.md)). The console's settings page for the group
lists the keys, to whoever configures it, with forms to add and take off
one. These methods are not among an assistant's tools (see
[assistants](assistants.md)).

The PLC directory says which keys a group has, so each method reads the
group's last operation there, every time, rather than a copy in the
host's database. The host keeps only a note of who added each key and
when, to show beside it.

`addRecoveryKey` takes a `did:key`. The host writes a PLC update, signed
with its own key after the last operation, that changes only the
rotation keys. The new key goes after the existing recovery keys, which
keep their rank, and ahead of the host's. PLC takes five rotation keys
and one is the host's, so a group has at most four recovery keys.

`removeRecoveryKey` takes a key off the same way, and the rest keep
their order. Any holder of `group.configure` may take off any steward's
key, not only one they added. The host's own key cannot be removed.

The host makes one change to a DID at a time, from reading its last
operation to publishing the next, so two changes asked at once both
land, one after the other. An operation the directory still refuses,
as when a steward changed the document meanwhile, is answered
`InvalidOperation`, with what the directory said; asking again builds on
the document as it is now.

`listRecoveryKeys` lists every rotation key in the document, the highest
ranked first. Each is marked `host` for this host's key or `steward` for
any other, with who added it and when if the host added it. A key put on
outside the host, such as a key a steward added directly at PLC, shows
as `steward` with no one named. `hostHolds` says whether the host's key
is still on the document. When it is false, the group's identity is in
its stewards' hands: the host can still list the keys but cannot change
them.

Details:

- A key is a `did:key` of secp256k1 or P-256; anything else is
  `InvalidKey`.
- A key already on the document is `AlreadyAdded`; a fifth recovery key
  is `TooManyKeys`.
- Removing the host's key is `HostKey`; a key not on the document is
  `UnknownKey`.
- Adding and removing appear in the group's activity, with who did it.
- The full list of fields and errors is in [the API
  reference](../reference/api.md).

## Moving and deleting

When a group moves to another host, its recovery keys move with it,
still ahead of the new host's keys. The old host's key is dropped.

Deleting a group ends its DID with a PLC tombstone only when the host's
key is the only rotation key. With a recovery key on the document, the
host leaves the DID alone, so its holders can bring the group back
elsewhere. Deleting needs `group.configure`, or, while the group is
switched off, to have held it when the group was switched off (see
[groups](groups.md)). `deleteGroup` answers `tombstoned`, which says
whether the DID was ended.

The host publishes the tombstone first, then deletes the group's account
and data. If the directory cannot be reached or refuses it, nothing is
deleted and `deleteGroup` fails, so it can be asked again; a DID already
ended counts as ended. Deleting a group also forgets the host's notes of who added
which key.

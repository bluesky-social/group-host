# Recovery

A group's identity is a `did:plc`. Its PLC document lists rotation keys,
and anyone who holds one may change the document. They can point the
group at another host, or end the DID. When the host creates a group,
its own key is the only rotation key. So the host holds every group's
identity alone. A host that fails could take every group it hosts down
with it, and a host that misbehaves could take them. Recovery keys let a
group's people hold its identity too.

## The host's key

The host has one rotation key, a secp256k1 key it makes on its first
start and keeps in its secrets. It signs every group's first PLC
operation, which names only that key. With only that key on the
document, the host is the only one that can move a group, and nobody can
move it without the host. The host publishes operations to the PLC
directory at `PLC_URL` and reads them from there (see
[config](../reference/config.md)).

## Recovery keys

A steward is someone the group trusts with its identity, and a recovery
key is a rotation key that a steward holds. The steward makes the key
pair themselves and gives the host only the public half, as a `did:key`.
The host never sees the private half.

PLC ranks the rotation keys in a document. Within 72 hours, a key may
undo an operation that a lower-ranked key signed. The host places
recovery keys ahead of its own, which is always last. So a steward can
undo anything the host does to the group's identity within 72 hours.
They can also move the group to another host without the host's help
(see [moving](moving.md)).

The host doesn't offer to sign anything with a recovery key, since it
never holds one. A steward uses theirs with their own PLC tooling,
against the PLC directory.

Holding a key has nothing to do with holding a role. The group's roles
decide who may add or remove keys on the host. They don't decide who
holds which key. If an admin takes a steward's key off, the host signs
that change with its own key. That key ranks below the steward's, so the
steward can undo the removal within 72 hours.

Once a group's identity is out of the host's control, the host stops
changing it. Once the document doesn't name the host's key, the host
refuses every add and remove with `NotController`, whichever key it's
asked about. The same goes once the DID has been ended.

## Adding, listing and removing keys

Adding, listing and removing keys all need `group.configure`, the action
that lets a member change the group's settings (see
[membership](membership.md)). The console's settings for the group list
the keys on a page of their own, for anyone who configures it. There's
a dialog to add a key and another to confirm before taking one off.
These methods aren't among the MCP tools (see [MCP](mcp.md)).

The PLC directory says which keys a group has. So each method reads the
group's last operation there every time, instead of a copy in the host's
database. The host keeps only a note of who added each key and when, to
show beside it.

`addRecoveryKey` takes a `did:key`. The host writes a PLC update, signed
with its own key after the last operation, that changes only the
rotation keys. The new key goes after the existing recovery keys, which
keep their rank, and ahead of the host's. PLC takes five rotation keys
and one is the host's, so a group has at most four recovery keys.

`removeRecoveryKey` takes a key off the same way, and the rest keep
their order. Any holder of `group.configure` may take off any steward's
key, including one someone else added. The host's own key can't be
removed.

The host makes one change to a DID at a time, from reading its last
operation to publishing the next. So if two changes are asked for at
once, both land, one after the other. If the directory still refuses an
operation (say a steward changed the document in the meantime), the
host answers `InvalidOperation` with what the directory said. Asking
again builds on the document's current state.

`listRecoveryKeys` lists every rotation key in the document, highest
ranked first. Each is marked `host` for this host's key or `steward` for
any other. If the host added a key, the list shows who added it and
when. A key put on outside the host, such as one a steward added
directly at PLC, shows as `steward` with no one named. `hostHolds` says
whether the host's key is still on the document. When it's false, the
group's identity is in its stewards' hands, and the host can still list
the keys but can't change them.

Details:

- A key is a `did:key` of secp256k1 or P-256. Anything else fails with
  `InvalidKey`.
- Adding a key that's already on the document fails with
  `AlreadyAdded`, and adding a fifth recovery key fails with
  `TooManyKeys`.
- Removing the host's key fails with `HostKey`, and removing a key
  that isn't on the document fails with `UnknownKey`.
- Adding and removing appear in the group's activity, with who did it.
- The full list of fields and errors is in [the API
  reference](../reference/api.md).

## Moving and deleting

When a group moves to another host, its recovery keys move with it,
still ahead of the new host's keys. The old host's key is dropped.

Deleting a group ends its DID with a PLC tombstone only when the host's
key is the only rotation key. With a recovery key on the document, the
host leaves the DID alone, so its holders can bring the group back
elsewhere. Deleting needs `group.configure`. While the group is switched
off, you need to have held it when the group was switched off (see
[groups](groups.md)). `deleteGroup` answers `tombstoned`, which says
whether the DID was ended.

The host publishes the tombstone first, then deletes the group's account
and data. If the directory can't be reached or refuses it, nothing is
deleted and `deleteGroup` fails, so you can ask again. A DID that's
already ended counts as ended. Deleting a group also forgets the host's
notes of who added which key.

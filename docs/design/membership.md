# Membership

A group has no one person in charge. So who is in it, and what each member
may do, is recorded where every member's tools can check it. Getting in is
a decision the group makes, and getting out leaves no access behind. The
methods' parameters and errors are in [the API reference](../reference/api.md).

## Where membership is recorded

The group writes its roster in its members space,
`at://<group>/space/fyi.opensocial.members/self`. A space is a private
record store whose authority, here the group, decides who may read and
write it ([spaces](spaces.md)). Any member's tools can read the roster
there, and no one outside the group can. The group writes three kinds of
record:

- `fyi.opensocial.membership`, one per member, keyed by their DID: the
  `roles` they hold.
- `fyi.opensocial.role`, one per role, keyed by its id: its name,
  description, and whether it is a `default` or `bot` role.
- `fyi.opensocial.permissions`, one record: a grant per role, with the
  scopes it carries and the roles it may assign.

A member is anyone with a membership record. Every member holds `member`,
the role the members space lets read, and `assignRoles` refuses to take it
away. Who may write the members space is fixed; `updateSpace` will not
change it.

A member may also write an `fyi.opensocial.acceptance` record in their own
repo there. It is their side of membership, for apps that show only those
who have taken up their place. It changes nothing they may do. `getGroup`
reports it to them as `viewer.accepted`.

## Roles and what they grant

The host's methods ask about twelve actions, such as `invite`, `admit`,
`eject`, `role.assign` and `group.configure`. Each is answered by exactly
one scope, in atproto's permission syntax
(`rpc:fyi.opensocial.createInvite?aud=*`, `space:*?manage=create`). A
member may do what any of their roles grants. Roles add up, with no deny,
precedence or hierarchy, so every "may they?" is a lookup. A grant may
also carry `repo:<collection>` scopes, the collections of the group's
public repo its holders may write as the group.

Power over other members needs its own limit. A grant's `assignable` lists
the roles its holders may give or take, and whose holders they may eject.
It counts only through the grant that carries the action: whom a member
may assign comes from their roles granting `role.assign`, whom they may
eject from those granting `eject`, and what they may admit someone with
from those granting `admit`.

A new group has two roles. `admin` grants every action and `repo:*`, and
may assign only `member`. `member` is the default role and grants nothing.
The founder and each steward named at creation hold both
([groups](groups.md)). So an admin cannot make or eject another admin
until a role's `assignable` says so.

Some member must always hold a role that grants `group.configure`, or no
one could run the group. Every method that could break this refuses with
`LastConfigurer`: `putRole` and `deleteRole`, and `assignRoles`,
`ejectMember` and `leaveGroup`.

Details:

- `putRole` and `deleteRole` need `group.configure`; `deleteRole`
  refuses a role someone holds.
- `assignRoles` (needs `role.assign`) sets a member's whole set of roles,
  each defined and each one the caller may assign.

## Joining

The profile's `joinPolicy` decides how people get in: `open`, `approval`
(the default) or `invite`. Someone asks with `requestJoin`. With `open`
they are in at once. With `invite` they are refused (`InviteRequired`)
unless invited. With `approval` their request waits, with an optional
message, until someone holding `admit` calls `admitMember` or
`denyJoinRequest`. A live invite admits whatever the policy, since the
group has already said yes.

A newcomer gets the default roles: those marked `default`, or `member` if
none is. An admitter may name other roles, but only ones they may assign.

The asker is no member, and the members space is for members, so requests
are not records: the host keeps them in its database, one per group and
asker. A decided request is deleted, so a denial looks like no request and
the asker may ask again.

Details:

- Asking again while a request waits keeps the first message and time.
- Getting in by any route ends a waiting request; `admitMember` then
  answers `AlreadyMember`.
- The asker sees their request with `getJoinRequest` and withdraws it
  with `cancelJoinRequest`; admitters list them with `listJoinRequests`.

## Invites

An invite must reach someone outside the group, so the group writes it
where they already look: their inbox,
`at://<invitee>/space/fyi.opensocial.inbox/self`, a space of their own at
their PDS. The invite is an `fyi.opensocial.invite` record in the group's
repo there, with `by`, an optional `message` and `expiresAt`. The host
answers `com.atproto.managedspace.checkUserAccess` for inboxes: only a
group on this host may write one, and only its owner may read it.

The group tells the invitee's PDS of the write at once. If that fails it
retries, the wait doubling from 10 seconds up to an hour, and gives up
after a day.

The inbox is the invitee's to read, so the host keeps its own list of
invites sent, one per group and invitee, and redeems from that list.
Redeeming takes the invite back, from the inbox and the list. Inviting
again answers the waiting invite; one past its `expiresAt` no longer
admits, and inviting again replaces it. The invitee may refuse with
`declineInvite`, and the group may still invite them later. Holders of
`invite` list invites with `listInvites` and take one back with
`revokeInvite`. A site that cannot read someone's inbox shows them their
invites through `getActorInvites`.

## Leaving and being put out

A member leaves with `leaveGroup`. A holder of `eject` puts one out with
`ejectMember`, if every role the member holds is one the ejecter's `eject`
grants may assign. Either way the host, in order:

1. revokes the space credentials the group issued them;
2. ends the OAuth grants they approved for the group, and their
   [assistants'](assistants.md) grants, with every session those began;
3. drops their app passwords for the group;
4. deletes their membership;
5. unindexes their repo in each of the group's spaces.

Credentials go first, so that if revoking fails they are still in and the
call can be made again. A credential lasts ten minutes; the group keeps
each it issues until it lapses, to revoke it at once. For writers on other
PDSes it sends `com.atproto.space.notifyCredentialRevoked`, best effort.
With no membership they hold no roles, so the access records grant them
nothing.

What they wrote stays in their own repos at their own PDSes. The group's
index forgets it at once, and drops on each sync any writer a space no
longer lets write.

# Membership

A group has no one person in charge. So the host records who's in it,
and what each member may do, where every member's tools can check it.
The group decides who gets in, and someone who leaves keeps no access.
The methods' parameters and errors are in [the API reference](../reference/api.md).

## Where membership is recorded

The group writes its roster in its members space,
`at://<group>/space/fyi.opensocial.members/self`. A space is a private
record store whose authority (here the group) decides who may read and
write it ([spaces](spaces.md)). Any member's tools can read the roster
there, and no one outside the group can. The group writes three kinds of
record:

- `fyi.opensocial.membership`, one per member, keyed by their DID. It
  holds the `roles` they have, each by its id (`["member", "admin"]`),
  at most 100.
- `fyi.opensocial.role`, one per role, keyed by its id. It holds the
  role's name, description, and whether it's a `default` or `bot` role.
- `fyi.opensocial.permissions`, one record at `self`. It holds a grant
  per role, with the scopes it carries and the roles it may assign
  (`defs#groupGrant`, each a list of strings).

Anyone who may read the members space can see who's in it with
`listMembers`. It answers each member and their roles, named by the
handle their DID document gives, a page at a time in DID order. A roster
outgrows one answer, so `getGroup` only counts them (`memberCount`).
With `q`, the page holds only members whose handle or DID contains it,
ignoring case. That's how the console finds a member as you type their
handle.

A member is anyone with a membership record. Every member holds
`member`, the role the members space lets read, and no membership leaves
it out. Who may read the members space is fixed, and `updateSpace` won't
change it.

Apps change these records the way they change any of the group's, by
writing them as the group with `com.atproto.space.*` (see the next
sections). The host has no method of its own for roles, grants or
memberships.

A member may also write an `fyi.opensocial.acceptance` record in their
own repo there. It's their side of membership, for apps that show only
people who have taken up their place. It doesn't change anything they
may do. `getGroup` reports it to them as `viewer.accepted`.

## Roles and what they grant

The host's methods ask about eleven actions, such as `invite`, `admit`,
`eject`, `role.assign` and `group.configure`. Each is answered by exactly
one scope, in atproto's permission syntax
(`space:fyi.opensocial.invites?authority=*&collection=fyi.opensocial.invite`,
`space:*?manage=create`). A
member may do what any of their roles grants. Roles add up, with no
deny, precedence or hierarchy, so every "may they?" is a lookup. A grant
may also carry `repo:<collection>` scopes, the collections of the
group's public repo its holders may write as the group.

The console takes what each person may change from `getGroup`. Its
settings are for a member whose roles grant `group.configure`, or who
may make app passwords (a grant of `repo:*`). It shows those members only
the parts that are theirs. Anyone else sees no way to the settings, and
an address under them says there's nothing there for them to change.

Power over other members needs its own limit. A grant's `assignable`
lists the roles its holders may give or take, and whose holders they may
eject. It counts only through the grant that carries the action. A
member's roles granting `role.assign` say whom they may assign, their
roles granting `eject` say whom they may eject, and their roles granting
`admit` say what roles they may admit someone with.

The actions over members are writes of memberships in the members space,
and their scopes say so. `admit` is
`space:fyi.opensocial.members?skey=self&collection=fyi.opensocial.membership&action=create`,
`role.assign` the same with `action=update`, and `eject` with
`action=delete`. A scope naming every write of memberships, or of every
collection in every space, carries all three. So does the
`fyi.opensocial.manageMembers` permission set, along with `invite`.

A new group has two roles. `admin` grants every action and `repo:*`, and
may assign `admin` and `member`. `member` is the default role and grants
nothing. The founder and each steward named at creation hold both
([groups](groups.md)).

Some member must always hold a role that grants `group.configure`, or
no one could run the group. The host refuses any write that would leave
nobody holding one with `LastConfigurer`, and refuses `leaveGroup` for
the last holder the same way.

## Writing roles, grants and memberships

An app writes these records as the group through `createRecord`,
`putRecord`, `deleteRecord` or `applyWrites` of `com.atproto.space.*`,
with a session for the group ([signing in](signing-in.md)). The
session's scope must name each write. At the time of each write, the
member who signed the app in must hold:

- `group.configure` for roles and the permissions record.
- `admit` to create a membership, `role.assign` to change one, and
  `eject` to delete one.

The host then makes each write only if it keeps the group whole, and
does what the write means. In one `applyWrites`, each write is checked
against the records as the earlier writes left them. It's checked
against the member's roles as they were before any of the writes. The
writes all happen together or not at all.

A role (`fyi.opensocial.role`) is keyed by its id, 64 characters at
most. It can only go once no member holds it and the permissions record
grants it nothing (`RoleInUse`). So an app deletes the role and writes
the permissions record without its grant at once. Its name is 64
characters at most, and its description 2,000.

The permissions record is kept at `self` and never deleted. It holds at
most one grant per role, for a role the group defines. In it a member
changes only:

- the grants of roles their own grants may assign, or of a new role that
  nobody holds and no grant names (`NotAssignable`).
- scopes their own roles hold, as written or within one they hold. A
  permission set of ours counts as the scopes it lists (`ScopeNotHeld`).
- `assignable` lists, adding only roles they may assign themselves, or a
  new role (`NotAssignable`), each defined.

So nobody gives a role more than they hold. And the founder can grant a
role they've just made and add it to what their own role may assign.

A membership (`fyi.opensocial.membership`) is kept at its member's DID,
holds `member` (`MemberRole`), and names only roles the group defines.

- Creating one admits someone waiting to join, and their request goes
  away ([joining](#joining)). Each role it gives must be one the
  writer's `admit` grants may assign. Nobody who hasn't asked can be
  made a member this way (`JoinRequestNotFound`).
- Writing one for a member changes their roles. Each role added or taken
  away must be one the writer's `role.assign` grants may assign. Any
  request of theirs that's still waiting goes away.
- Deleting one puts the member out, if each role they hold is one the
  writer's `eject` grants may assign ([below](#leaving-and-being-removed)).

A token from `getGroupAuth` for the members space writes what the
member's grants let them write there. The console's member and settings
pages write with one, and so do the `opensocial` commands. An
[MCP client](mcp.md) makes the same writes with its tools, as the member
who connected it: `role_set`, `role_delete`, `role_grant`,
`member_assign` and `member_eject`.

## Joining

The profile's `joinPolicy` decides how people get in: `open`, `approval`
(the default) or `invite`. Someone asks with `requestJoin`. With `open`
they're in at once. With `invite` the host refuses them (`JoinClosed`)
unless they're invited. With `approval` their request waits, with an
optional message, until someone holding `admit` writes their membership
as the group or turns them down with `denyJoinRequest`. A live invite
admits them under any policy, since the group has already said yes. The
asker may name it (`invite`, its at-uri). If the named invite isn't
their live invite from this group, the host refuses it (`InvalidInvite`).
If they don't name one, a live invite admits them all the same.

`requestJoin` answers `status`. That's `admitted` with the `membership`
it wrote (its `uri` and `cid`), or `pending` with the `request` waiting.

Someone let in by `requestJoin`, or by an invite, gets the default
roles. Those are the roles marked `default` (or `member` if none is),
and always `member`. An admitter writing a membership gives the roles it
names, each one the group defines and they may assign.

The asker isn't a member, and the members space is for members, so
requests aren't records. The host keeps them in its database, one per
group and asker. It deletes a request once it's decided, so a denial
looks like no request and the asker may ask again.

Details:

- Asking again while a request waits keeps the first message and time.
- A request's message is 2,000 characters at most.
- Getting in by any route ends a waiting request, and so does any write
  of the asker's membership.
- The asker sees their request with `getJoinRequest` and withdraws it
  with `cancelJoinRequest`. Admitters list them with `listJoinRequests`.

## Invites

An invite must reach someone outside the group, so the group writes it
where they already look. That's their inbox,
`at://<invitee>/space/fyi.opensocial.invites/self`, a space of their own
at their PDS. The invite is an `fyi.opensocial.invite` record in the
group's repo there, with a `createdAt` and an optional `message` and
`expiresAt`. An inbox's policy is `com.atproto.managedspace.policy`,
which names this host as its managing app (`<host DID>#opensocial`). So
the host answers `com.atproto.managedspace.checkAccess` for it, when the
inbox's authority asks with service auth. A group on this host may write
one and its owner may read it. With no account named, nobody may do
either.

An app invites someone by writing that record as the group, through
`com.atproto.space.createRecord` (or `putRecord`, or `applyWrites` with
one write) with a session for the group. It takes the invite back by
deleting it (`deleteRecord`). A session gets there through the
`manageMembers` permission set, a `space:fyi.opensocial.invites` scope,
or a group token for the inbox from `getGroupAuth`, which the console
uses ([signing in](signing-in.md)). In every case, the member who signed
the app in must hold `invite` at the time of each write. The host checks
each write the same way it would check its own:

- An inbox holds invites and nothing else, at `self`.
- The invitee may not be a member already (`AlreadyMember`).
- A group has one invite to each person at a time. Writing one while one
  waits answers the waiting one, unchanged, unless it's written at that
  one's key, which replaces it.
- The host refuses to delete an invite that isn't waiting
  (`InviteNotFound`).

The group tells the invitee's PDS about the write at once. If that
fails, it retries with the wait doubling from 10 seconds up to an hour,
and gives up after a day.

The inbox is the invitee's to read. So the host keeps its own list of
invites sent, one per group and invitee, kept in step with each write,
and redeems from that list. Redeeming takes the invite back from the
inbox and the list once the membership is written, so a join that fails
leaves the invite waiting. An invite past its `expiresAt` doesn't admit
anyone. The host takes it back when the invitee asks to join, or on its
own within the hour, and inviting again writes a fresh one. The invitee
may refuse with `declineInvite`, and the group may still invite them
later. Holders of `invite` list invites with `listInvites`.

A site that can't read someone's inbox shows them their invites through
`getActorInvites`. The console reads them once for each page, beside who
is signed in. A dot on your picture in the header and a count in its
menu say some are waiting. The home page lists them, each with a way to
join or decline, and the group's own page does the same for an invited
person who opens it. Declining one doesn't ask you to confirm, since the
group can send another.

Details:

- `expiresAt` is an atproto datetime, in UTC or at an offset from it,
  and must be in the future.
- An invite's message is 2,000 characters at most.

## Leaving and being removed

A member leaves with `leaveGroup`. A holder of `eject` puts one out by
deleting their membership as the group, if every role the member holds
is one the ejecter's `eject` grants may assign. Either way the host:

1. revokes the space credentials the group issued them.
2. ends the OAuth grants they approved for the group, and their
   [MCP clients'](mcp.md) grants, with every session those began.
3. drops their app passwords for the group.
4. deletes their membership.
5. takes them out of the writer set of each of the group's spaces. The
   index forgets what they wrote there, and the services syncing each
   space are told (`repoRemoved`).

Credentials go first. So if revoking fails, they're still in, and the
call (or the write deleting their membership) can be made again. Once
they're removed, the rest follows the write. Deleting a membership that's
already gone changes nothing and does nothing more. A credential lasts
ten minutes, and the group keeps each one it issues until it lapses, so
it can revoke it at once. For writers on other PDSes it sends
`com.atproto.space.notifyCredentialsRevoked`, best effort. With no
membership they hold no roles, so the access records grant them nothing.

The group then keeps an ejection in its [moderation](moderation.md)
queue, as an `ejectEvent` on the member's DID, with who removed them.
Leaving keeps nothing there.

What they wrote stays in their own repos at their own PDSes. The group's
index forgets it at once. On each sync it drops any writer that a space
doesn't let write anymore.

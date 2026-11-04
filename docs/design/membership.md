# Membership

Who is in a group, and what each may do, are records in its members
space, written by the group: a membership for each member, naming their
roles; a role record for each role, its name and words; and one
permissions record, granting each role OAuth scopes and the roles its
holders may give. The host's methods ask by action (`invite`, `admit`,
`role.assign`, `eject`, `group.configure`, …), each answered by the one
scope that grants it, and roles compose by union (`rbac.rs`).

## Roles

- `putRole` (for `group.configure`) defines a role or changes one: its
  name and words; whether its people are bots; whether every new member
  gets it (kept when not said); what it may do, as the scopes of its
  actions beside any other scope it held; its public-repo collections
  (kept when not said); and the roles its holders may give.
- `deleteRole` (for `group.configure`) takes away a role nobody holds
  (`RoleInUse` while someone does).

A group never loses its last way to be run: the last role that may
configure it keeps that (`LastConfigurer`), and stays.

## Members

- `assignRoles` (for `role.assign`) gives a member the roles named in
  place of theirs. Each role added or taken away must be one the
  caller's roles may give; each must be defined; the member must be
  one. Everyone in a group holds `member`, which lets them read the
  members space: it goes only when they do (`MemberRole`).
- `ejectMember` (for `eject`) puts a member out, if the caller's roles
  may give or take each of theirs.
- `leaveGroup` takes the caller out. The last member able to configure
  the group may not leave (`LastConfigurer`).

## Joining

How people get in is the group's profile's `joinPolicy`:

- `open`: `requestJoin` admits them at once, with the roles the group
  marks default (`member`, unless it marks others).
- `approval`: `requestJoin` keeps their request, with what they say, for
  one holding `admit`. `listJoinRequests` lists those waiting, newest
  first; `getGroup` shows them to whoever may admit, and to the asker
  their own. `admitMember` admits one, with the defaults or the roles
  named, each one the admitter may give (admitting gives roles, so it
  is bound as giving them is); `denyJoinRequest` turns one down;
  `cancelJoinRequest` withdraws one's own; `getJoinRequest` is one's
  own, while it waits.
- `invite`: not without an invite (`InviteRequired`).

An invite admits its invitee whatever the policy: asking to join
redeems it, with the group's default roles.

A request waits in the host, not in the group's records: who asks is no
member, and the members space is for members. Decided, it goes: a
request turned down looks the same as one never made, so its asker may
ask again and the group need not say.

## Invites

A group invites someone by writing an invite in its own repo in their
inbox: a space under their own DID (`fyi.opensocial.inbox`), at their
PDS, whose policy lets this host say who reads and writes it. The host
answers its authority (`com.atproto.managedspace.checkUserAccess`, with
service auth from the authority): its owner reads it, and only groups on
this host write in it. The group's write is told to the invitee's PDS as
any write to a space elsewhere is, owed until it lands. The invitee's
apps read the invite there; the host keeps its own list of what it sent,
which is what redeems an invite, as the inbox is its owner's to read,
not the group's.

- `createInvite` (for `invite`) invites someone not yet a member, with
  words and a lapse if it says; one already waiting is the one answered.
- `listInvites` (for `invite`) lists those waiting, newest first, the
  lapsed left out.
- `revokeInvite` (for `invite`) takes one back by its URI; `declineInvite`
  is its invitee's own say. Either takes the record out of the inbox and
  the host's list; the group may invite them again.
- `getActorInvites` is the caller's own invites waiting, from the groups
  here, for a group's site, which can't read the caller's inbox.

## Out is out

A member put out, or leaving, reads the group's spaces no more:

- their membership goes, so the access records let them read nothing
  that needs a role;
- the credentials the group issued them are revoked: at once for the
  repos on this host, and told to the host of each other writer in
  those spaces (`notifyCredentialRevoked`, signed by the group, best
  effort), so they lapse everywhere rather than within the hour. The
  authority keeps each credential it issues, to whom, until it lapses,
  for this;
- the index forgets what they wrote, and keeps only writers the space
  still lets write.

What they wrote stays in their own repos, at their own PDSes.

## From the prototype

As `methods/roles.ts`, `methods/membership.ts` and `leaveGroup`, with
these changes: `assignRoles` refuses someone who is not a member, where
the prototype would write them a membership and so admit them; roles
given must be defined; `deleteRole` refuses a role someone holds, as
the lexicon's `RoleInUse` says; leaving or being put out revokes
credentials, which the prototype left to lapse; an inbox takes invites
from groups here alone, where the prototype let anyone write in it; and
an invite's `expiresAt` is kept to.

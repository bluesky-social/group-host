# Groups

A group is an atproto account that no one person owns. It has no password
and no one to sign in as it, yet someone has to start it, change its
profile and rules, give it a name, and one day end it. On this host each
of those is decided by the roles its members hold. What a group shows
also depends on who asks: a stranger, a member and an admin each see a
different amount. The inputs and errors of every method named here are in
[the API reference](../reference/api.md).

## A group is an account with two spaces

A group needs a place for what it is and a place for who belongs to it,
and some of that only members should read. So each group is an account of
its own, with its own `did:plc` and public repo, and this host as its PDS.
It also gets two spaces. A space is a private record store: each member
writes in their own repo in it, on their own PDS, and the space's
authority decides who may read and write. The group is the authority of
both of its spaces; [spaces](spaces.md) explains them.

- `meta` (`at://<group>/space/fyi.opensocial.meta/self`) holds the
  profile, the rules and the labeler.
- `members` (`at://<group>/space/fyi.opensocial.members/self`) holds
  roles, permissions, memberships, and the index of the group's spaces.

Everything a group is lives in records the group writes in its own repos
in those spaces, and the host reads them to answer for it. Something is a
group here only if it is an account on this host and its `members` space
exists here. Outside its records the host keeps the account and its keys,
its address, the stewards kept while it is switched off, join requests,
invites and tokens.

`createGroup` starts a group, and its caller becomes the founder and first
admin. A caller signs in with service auth from their own PDS (a
short-lived token for calling this host as them) or, from the console, by
its session cookie; every signed-in method here takes either.
`GROUP_FOUNDERS` names the people the host starts groups for, or anyone
when empty ([configuration](../reference/config.md)).

Starting a group writes its profile (display name, description, join
policy), labeler and access record in `meta`. In `members` it writes an
access record readable by `member`, the roles `admin` and `member`, the
permissions record, index entries for `meta` and `members`, and a
membership with both roles for the founder and each steward, the people
named at the start to run the group beside the founder. If anyone may
read `meta`, it writes a declaration in the public repo. If setup fails
partway, the host deletes the account and ends its DID, so the handle is
free again. Joining and roles are in [membership](membership.md).

Details:

- The join policy is `open`, `approval` or `invite`; `approval` when not
  given.
- A membership's record key is the member's DID.
- Text is held to its lexicon's length, here and in every method that
  writes it, and refused with `InvalidRequest` past it: a group's display
  name 64 characters, its description 300. The host counts characters,
  so a text of combined characters may be refused a little short of the
  lexicon's grapheme limit.

## Its address

A group needs a name before it has a domain of its own. The host gives it
a handle of one label under `HANDLE_DOMAIN`, the domain of this host's
handles, and keeps that handle as the group's address. The address never
changes. When the group takes a handle on its own domain, the address
stays reserved for it, until the group is deleted.

`checkAddress` says, without signing in, whether an address is free: not
an account's handle here and not kept as a group's address.
`describeServer` names the domain that follows the label. The console's
new-group form builds the label from the name as typed, asks
`checkAddress` once typing stops, and when the address is taken offers the
first free one of `<label>-2` to `<label>-19`.

## Roles decide what each person may do

With no owner to ask, "may this person do that?" is answered from the
group's records.

A member's roles are the `roles` list of their membership in `members`. A
space's access record says who reads it (`readableBy`) and who writes in
it in their own name (`writableBy`, any member when absent), as lists of
roles; `public` among them means anyone signed in. An access record can
instead name `people`, the chosen people a space is for, with `reviewers`,
the roles that may also read it.

The permissions record in `members` grants each role OAuth scopes and the
roles it may assign. Each of the host's twelve actions, such as `invite`,
`eject`, `space.create` or `group.configure`, is answered by one scope.
Roles compose by union: a person may do whatever any of their roles
grants, with no deny, precedence or hierarchy. The same scopes bound an
app or assistant a member signs in ([signing in](signing-in.md)).

As made, `admin` holds every action, may write any public-repo collection
as the group, and may assign `member` but not `admin`. `member` holds
nothing. A group changes this by changing its permissions record.

The host, as authority, decides every read and write in a group's spaces.
The group itself always may. Anyone else may if their roles fit the
space's access record, or if they hold an explicit grant. Nobody reads or
writes in the spaces of a group that is switched off.

Details:

- An access record's `credentialScopes` say which collections each role
  writes in that space as the group; in `meta`, `admin` writes the profile
  and rules.
- A missing action is refused with `403 Forbidden`, "missing <action>".

## What each person sees, and finding a group

`getGroup` answers anyone and gives more to those allowed more. Everyone
gets the group's DID, handle, address, and the URIs of its two spaces.
Whoever may read `meta` also gets the profile and the rules, in order. A
signed-in caller gets `viewer`: whether they are a member, their roles and
actions, and for a member whether their own repo in `members` holds
anything yet. Whoever may read `members` gets the members, the roles and
what each may do, and the spaces, with a space's access shown only to
those who may read it. A holder of `admit` also gets the join requests. A
switched-off group shows only the first part and `deactivated: true`.

A group that wants to be found says so with its declaration, a
`fyi.opensocial.declaration` record in its public repo pointing at
`meta`. The host writes it when `public` is among `meta`'s readers and
removes it when not. `listGroups` lists every group on the host, adding
the profile and rules of those whose `meta` is readable by `public`. A
hidden group still appears with its DID, handle, address and space URIs.

`getActorGroups` lists the switched-on groups a person is in that the
caller is in too, with each group's member count, and its display name and
avatar when the caller may read `meta`. `getActor` describes a member of
a group here, or someone asking to join one, to a signed-in caller.

## Changing its profile, rules and pictures

The host writes a group's profile and rules in `meta`, as the group, for
anyone whose roles hold `group.configure`. `updateProfile`, `uploadImage`,
`putRule` and `deleteRule` all ask for it. Nothing is written as a
switched-off group.

`updateProfile` replaces the profile. A display name is required. The
description, site, location, online flag, topics and languages are as
given, and removed when left out. The join policy, avatar and banner are
kept when left out; `removeAvatar` and `removeBanner` clear them.

A picture must be a blob in the group's own repo, because a blob in
another account's repo would not resolve for the group's profile. So
`uploadImage` keeps the image's bytes as the group's blob, and the client
names that blob in `updateProfile`. An upload no record takes within a
day is swept away, and a blob goes when its last reference does.

The host serves images at `/img/{did}/{cid}` to whoever may see a record
holding them: anyone, for the public repo or for `meta` while the group is
declared; otherwise whoever may read the space, signed in to the console.
A switched-off group's images are not served.

`putRule` adds or changes a rule and `deleteRule` removes one. Rules keep
an order, and a new rule goes last. A label citing a deleted rule keeps
its URI, which no longer resolves ([moderation](moderation.md)).

Details:

- `uploadImage` takes at most 1,000,000 bytes, of a PNG, JPEG, WebP, GIF
  or AVIF image as its `Content-Type` says, and refuses any other type.
- `updateProfile` takes a picture's type and size from the upload the
  host kept, not from what the caller writes, and refuses a blob that is
  not one of the group's images.
- A topic is cut to 64 characters, and topics the cut makes the same are
  kept once.
- A new rule's record key is a TID from the host's clock. `putRule` with
  an `rkey` changes the rule of that key, and refuses a key no rule has.
- A rule's title is 100 characters at most, its text 2,000.
- The console's settings set pictures through `uploadImage` then
  `updateProfile`, and visibility by setting `meta`'s readers to `public`
  or `member` through `updateSpace`.

## More spaces

A group may want more places, such as one only its admins read. Each
further space has an access record and an index entry in `members` that
apps read to find it. `createSpace` (for `space.create`) makes one from a
type, its readers, and optionally its writers, key, name and credential
scopes. A space is made whole or not at all: if its access record or
its index entry cannot be written, the host ends it again, and its key
is free to use. `updateSpace` (for `space.configure`) changes those,
and changes nothing when it cannot change them all. Both refuse a
`readableBy` that names nobody, and a name over 64 characters.
`deleteSpace` (for `space.delete`) removes the index entry, tells the
space's syncers, and deletes the space and the group's repo in it. What
members wrote there stays theirs at their PDSes.

Two spaces carry the group itself, so their rules are fixed: `meta` and
`members` cannot be deleted, `members` must stay readable by `member`, and
who joins it by writing there cannot change. A space for chosen people is
theirs, and the group can neither change nor delete it.

## A handle on its own domain

A group may take a handle on a domain it controls. The domain must name
the group the way any handle names an account: by
`/.well-known/atproto-did`, or through the services in `HANDLE_RESOLVERS`.
Under `HANDLE_DOMAIN` a group may take only its own address.

`checkGroupHandle` says whether a handle names the group yet, changing
nothing. `updateGroupHandle` checks the same, holds the handle in the
host's own record, updates the group's DID document with the host's
rotation key, and tells relays. If the directory refuses the update, the
host puts the group's previous handle back. It fails when the domain does not name the group, or when
the host's key no longer controls the DID. Both are for `group.configure`.

## Switching off, on and deleting

`deactivateGroup` (for `group.configure`) switches a group off and tells
relays. While it is off, its repos refuse reads, its spaces grant nothing
to anyone, the group included, and its images are not served.

A switched-off group's roles cannot decide who may switch it back on, so
the host keeps who held `group.configure` when it was switched off.
`activateGroup` is for one of those stewards.

`deleteGroup` is for a holder of `group.configure`, or for a kept steward
while the group is off. It tells the syncers of each space, ends the
DID, then deletes the account, repos, spaces, address, invites, join requests, tokens,
moderation state and blobs together, and tells relays. What members wrote
in its spaces stays theirs, at their PDSes.

The host ends the DID with a tombstone only when its own key is the only
rotation key on it. If a steward holds a recovery key there, the host
leaves the DID for its holder to bring the group back elsewhere, and
`deleteGroup` answers `tombstoned: false` ([recovery](recovery.md),
[moving](moving.md)). If the directory cannot be reached or refuses the
tombstone, `deleteGroup` fails and the group stays as it was, to be
deleted again.

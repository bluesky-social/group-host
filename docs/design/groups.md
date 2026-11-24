# Groups

A group is an atproto account that no one person owns. It has no password
and nobody signs in as it. But someone still has to start it, change its
profile and rules, give it a name, and one day end it. On this host the
roles its members hold decide each of those. What a group shows also
depends on who asks, so a stranger, a member and an admin each see a
different amount. The inputs and errors of every method named here are in
[the API reference](../reference/api.md).

## A group is an account with two spaces

A group needs a place for what it is and a place for who belongs to it,
and some of that only members should read. So each group is an account of
its own, with its own `did:plc` and public repo, and this host as its PDS.
It also gets two spaces. A space is a private record store. Each member
writes in their own repo in it, on their own PDS, and the space's
authority decides who may read and write. The group is the authority of
both of its spaces, and [spaces](spaces.md) explains them.

- `meta` (`at://<group>/space/fyi.opensocial.meta/self`) holds the
  profile, the rules and the labeler.
- `members` (`at://<group>/space/fyi.opensocial.members/self`) holds
  roles, permissions, memberships, and the index of the group's spaces.

Everything a group is lives in records the group writes in its own repos
in those spaces, and the host reads them to answer for it. Something is a
group here only if it's an account on this host and its `members` space
exists here. Outside its records, the host keeps the account and its
keys, its address, the stewards it holds on to while the group is
switched off, join requests, invites and tokens.

`createGroup` starts a group, and its caller becomes the founder and first
admin. A caller signs in with service auth from their own PDS (a
short-lived token for calling this host as them) or, from the console,
with its session cookie. Every signed-in method here takes either.
`GROUP_FOUNDERS` names the people the host starts groups for. When it's
empty, the host starts them for anyone
([configuration](../reference/config.md)).

Starting a group makes its `meta` and `members` spaces, each under a
policy, and writes its profile (display name, description, join policy)
and labeler in `meta`. In `members` it writes the roles `admin` and
`member` with what each may do. The host publishes each space's policy
there and lists both spaces in the index in `members`. It also
writes a membership with both roles for the founder and for each
steward. Stewards are the people named at the start to run the group
beside the founder. If anyone may read `meta`, the host writes a
declaration in the public repo. If setup fails partway, the host deletes
the account and ends its DID, so the handle is free again. Joining and
roles are in [membership](membership.md).

Details:

- The join policy is `open`, `approval` or `invite`. It's `approval`
  when not given.
- A membership's record key is the member's DID, and it names nobody
  else.
- Text is held to its lexicon's length, here and in every method that
  writes it. Past that, the host refuses it with `InvalidRequest`. A
  group's display name is 64 graphemes at most, and its description
  2,000.

## Its address

A group needs a name before it has a domain of its own. So the host gives
it a handle of one label under `HANDLE_DOMAIN`, the domain of this host's
handles. It keeps that handle as the group's address, where its site is
served ([sites](sites.md)). The address never changes. When the group
takes a handle on its own domain, the address stays reserved for it, and
its site stays there, until the group is deleted.

`checkAddress` says, without signing in, whether an address is free. A
free address isn't an account's handle here and isn't kept as a group's
address. `describeServer` names the domain that follows the label. The
console's new-group form builds the label from the name as you type it,
and asks `checkAddress` once you stop typing. When the address is taken,
it offers the first free one of `<label>-2` to `<label>-19`.

## Roles decide what each person may do

There's no owner to ask, so the group's records answer "may this person
do that?"

A member's roles are the `roles` list of their membership in `members`. A
space's policy, `fyi.opensocial.policy`, gives each role a grant there,
one entry a role. A role granted `read` reads the space, and one granted
`write` writes there in its members' own names. A role granted a space
scope naming collections writes those as the group there. The policy
may also set `publicRead`, so anyone at all may read the space, signed
in or not ([spaces](spaces.md#deciding-who-reads-and-writes)).

Each role's record in `members` grants it OAuth scopes across the group
and names the roles it may assign, `*` for every role. One scope answers
each of the host's eleven actions, such as `invite`, `eject`,
`space.create` or `group.configure`. Roles compose by union. A person
may do anything any of their roles grants, and there's no deny,
precedence or hierarchy. The same scopes bound an app or MCP client a
member signs in ([signing in](signing-in.md)).

When a group is made, `admin` holds every action, may write any
public-repo collection as the group, and may assign every role, those
made later too. `member` holds nothing. A group changes this by writing
its role records as the group
([membership](membership.md#writing-roles-and-memberships)).

The host, as authority, decides every read and write in a group's spaces.
The group itself always may. Anyone else may if the space's policy lets
their roles, or if they hold a grant of their own that came with the
space when it moved here. Nobody reads or writes in the spaces of a group
that's switched off.

Details:

- When a group is made, anyone may read `meta` unless `createGroup` sets
  `metaPublicRead: false`. Its policy grants `member` `read`, and `admin`
  `read` and
  `space:fyi.opensocial.meta?collection=fyi.opensocial.profile&collection=fyi.opensocial.rule`,
  so admins write the profile and rules as the group. The policy of
  `members` grants `member` `read` and `write`, so members' own records
  there sync.
- The host reads a grant's scopes the way it reads a token's. A scope
  naming another type, key or authority grants nothing in the space. One
  naming no collection writes nothing as the group.
- The host refuses a missing action with `403 Forbidden`, "missing
  <action>".

## What each person sees, and finding a group

If you aren't signed in, you learn only what anyone can already learn
from the group's DID document, which is its DID and handle. `getGroup`
tells you that and nothing more, even when anyone may read `meta`.

`getGroup` gives more to callers allowed more. Everyone signed in gets
the group's DID, handle, address, and the URIs of its two spaces. A
caller who may read `meta` also gets the profile and the rules, in order.
A signed-in caller gets `viewer`. It says whether they're a member, their
roles and actions, and for a member whether their own repo in `members`
holds anything yet. A caller who may read `members` gets how many
members it has (`listMembers` names them, a page at a time), the roles
and what each may do, and the spaces: those in the index, in its order,
then the unlisted ones the caller may read. A space's policy shows only
to callers who may read the space. A holder of `admit` also gets the
join requests.
A switched-off group shows only the first part and `deactivated: true`.
To a signed-in caller it also shows `viewer.steward`, which says whether
they may switch it back on, since the host reads nothing else of their
standing.

A group that wants to be found says so with its declaration. That's a
`fyi.opensocial.declaration` record in its public repo pointing at
`meta`. The host writes it while the policy of `meta` sets `publicRead`
and removes it when it doesn't. The host keeps no list of its groups for
people to browse. How groups are found across the network is still to
be decided, and the declaration is what such a list would read.

`getActorGroups` lists the switched-on groups a person is in that the
caller is in too. It gives each group's member count, and its display
name and avatar when the caller may read `meta`. `getActor` describes a
member of a group here, or someone asking to join one, to a signed-in
caller.

## Changing its profile, rules and pictures

A group's profile (`fyi.opensocial.profile`, at `self`) and its rules
(`fyi.opensocial.rule`) are records in `meta`. An app changes them by
writing them as the group, with a token for the group
([signing in](signing-in.md)). It uses `com.atproto.space.putRecord`,
`createRecord`, `deleteRecord` and `applyWrites`. The token's scope must
cover the write, and the member who approved it must hold
`group.configure` when it's made. Nothing is written as a switched-off
group.

A put replaces the record whole, so anything it leaves out goes. Beyond
what the lexicon checks, the host keeps a profile only if it has a
display name that's more than spaces and a join policy of `open`,
`approval` or `invite`. A site must be on `http` or `https`.
The profile stays at `self` and is never deleted.

A picture must be a blob in the group's own repo, because a blob in
another account's repo wouldn't resolve for the group's profile. So the
app uploads it as the group (`com.atproto.repo.uploadBlob`, which a
token for `meta` may do for images) and names the blob it gets back in
the profile. An upload no record takes within a day is swept away, and
a blob goes when its last reference does.

The host serves images at `/img/{did}/{cid}` to anyone who may see a
record holding them. For the public repo that's anyone. Otherwise it's
anyone signed in to the console who may read the space. For `meta` while
the group is declared, that's anyone signed in. A switched-off group's
images aren't served, nor is an image the group has taken down in a
space ([moderation](moderation.md#labels)).

A rule is kept at a TID, so a label citing it keeps its URI however the
rule changes. When a new rule is written with no key, the host's clock
gives it one. Rules are shown by their `order`, lowest first, then the
ones with none, in the order they were written. A label citing a deleted
rule keeps its URI, which doesn't resolve
([moderation](moderation.md)).

The declaration follows who may read `meta`. When it's written as the
group in its public repo, the host keeps it only at `self`, naming the
group's own `meta`, and only while anyone may read it. Deleting it is
always allowed.

Details:

- A picture is a PNG, JPEG or WebP of at most 2,000,000 bytes. Its
  record names it with the type and size the host kept at upload. The
  host refuses a record that says otherwise, or a blob that isn't the
  group's.
- A rule's title is 64 characters at most and must be more than spaces.
  Its text is required and is 2,000 characters at most.
- The console's settings write the profile, pictures and rules this way,
  with a token `getGroupAuth` gives for `meta`. They set visibility by
  changing `publicRead` in the policy of `meta` through
  `com.atproto.space.updateSpace`, and members read it either way.
- An MCP client changes them with the `profile_set`, `rule_set` and
  `rule_delete` tools, and you can with the `opensocial` commands of the
  same names. Both make the same writes as their member
  ([MCP](../reference/mcp.md)).

## More spaces

A group may want more places, such as one only its admins read. A
member makes, changes and ends them through atproto's own space
methods, `com.atproto.space.createSpace`, `updateSpace` and
`deleteSpace`, called as the group. Each further space is kept under a
policy, and listed in the index in `members` unless its policy sets
`unlisted`. How the host checks each call, and what it refuses, is in
[spaces](spaces.md#making-changing-and-ending-a-space).

Two spaces carry the group itself. `meta` and `members` can't be made
again, and nobody may delete them: they go when the group does.

## A handle on its own domain

A group may take a handle on a domain it controls. The domain must name
the group the way any handle names an account, by
`/.well-known/atproto-did` or through the services in `HANDLE_RESOLVERS`.
Under `HANDLE_DOMAIN` a group may take only its own address.

`checkGroupHandle` says whether a handle names the group yet, and changes
nothing. `updateGroupHandle` checks the same and holds the handle in the
host's own record. Then it updates the group's DID document with the
host's rotation key and tells relays. If the directory refuses the
update, the host puts the group's previous handle back. It fails when
the domain doesn't name the group, or when the host's key no longer
controls the DID. Both are for `group.configure`. The group's site stays
at its address whatever its handle is ([sites](sites.md)).

In the console, the profile page of a group's settings shows its handle.
While that's a domain of its own, it also shows whether the domain still
names the group. "Use your own domain" takes the domain and shows the
`_atproto` TXT record to add there, `did=` and the group's DID. "Go back"
gives the group its address back, after asking you to confirm. The
command is `group handle`, and an MCP client's tool is `group_handle`.

## Switching off, on and deleting

`deactivateGroup` (for `group.configure`) switches a group off and tells
relays. While it's off, its repos refuse reads, its spaces grant nothing
to anyone (the group included), and its images aren't served.

A switched-off group's roles can't decide who may switch it back on. So
the host keeps who held `group.configure` when it was switched off, and
`activateGroup` is for one of those stewards.

In the console, the profile page of a group's settings switches it off
or deletes it, and asks you to confirm each. When the group is off, its
page says so and offers its stewards to switch it back on or delete it.
The commands are `group deactivate` (`--undo` switches it back on) and
`group delete`. An MCP client can switch the group off, but it can never
delete it.

`deleteGroup` is for a holder of `group.configure`, or for a kept steward
while the group is off. It tells the syncers of each space and ends the
DID. Then it deletes the account, repos, spaces, address, invites, join
requests, tokens, moderation state and blobs together, and tells relays.
What members wrote in its spaces stays theirs, at their PDSes.

The host ends the DID with a tombstone only when its own key is the only
rotation key on it. If a steward holds a recovery key there, the host
leaves the DID for its holder to bring the group back elsewhere, and
`deleteGroup` answers `tombstoned: false` ([recovery](recovery.md),
[moving](moving.md)). If the directory can't be reached or refuses the
tombstone, `deleteGroup` fails and the group stays as it was, so you can
delete it again.

# Groups

A group is an account on this host: its own `did:plc`, its own public
repo, and two spaces of its own, of which it is the authority.

- **meta** (`at://<group>/space/fyi.opensocial.meta/self`): its profile,
  rules and labeler, for whoever it lets read them (anyone signed in,
  unless it says otherwise).
- **members** (`at://<group>/space/fyi.opensocial.members/self`): its
  roles, what each may do, its memberships and the index of its spaces,
  for its members.

Everything a group is lives in records the group writes in its own
repos there. The host reads them to answer for it; it keeps nothing of
a group beside them but the account and the spaces.

## Starting one

`fyi.opensocial.createGroup`, signed in as the founder with service auth
from their own PDS (for this host's `SERVICE_DID`), makes:

- the account, at a handle one label under `HANDLE_DOMAIN`;
- in meta, its profile (name, words, how people join: `open`,
  `approval` or `invite`), its labeler, and an access record: who reads
  meta, and that admins write the profile and rules as the group;
- in members, an access record (members read it), the `admin` and
  `member` roles (`member` given to all who join), the permissions record
  (`admin` may do every action, write any public-repo collection as the
  group, and give out `member`; `member` holds nothing more), an index
  entry for each space, and the founder's and stewards' memberships as
  admins and members;
- if anyone may read meta, a declaration in the group's public repo,
  pointing at it, so it can be found.

`GROUP_FOUNDERS` names who the host starts groups for; anyone, unless it
names someone.

## Who may do what

A space's access record says who reads it (`readableBy`: roles, or
`public` for anyone signed in) and who writes in it in their own name
(`writableBy`; any member, when it says no one), or names the chosen
people it is for. A member's roles are their membership record in the
members space. The permissions record grants each role OAuth scopes;
each action the host's methods ask about (`invite`, `admit`,
`role.assign`, `group.configure`, …) is answered by one scope. Roles
compose by union.

The host, as each space's authority, issues credentials and admits
writes by those records (`rbac.rs`): the same answer whether it reads a
group for someone or lets them read it themselves.

## Reading one

- `fyi.opensocial.getGroup` is a group as its caller may see it: its
  profile and rules for whoever may read meta, their own standing (are
  they a member, their roles, the actions they have, have they written
  their acceptance), and for a member, its members, roles, what each
  role may do, and its spaces, each with its access record for whoever
  that lets read the space.
- `fyi.opensocial.listGroups` is every group here, unauthenticated: by
  handle, and for one whose meta anyone may read, its profile and rules.

## Changing its face

Its profile, images and rules are records in meta written by the group,
so the host writes them for a caller holding `group.configure`:

- `updateProfile` replaces the profile. Where it meets, whether online,
  its topics and languages are as given, none when left out (topics
  trimmed, each once, ten at most, 64 characters each; languages BCP 47
  tags, five at most). Its site and images are kept when left out, and
  `null` clears an image. Its first `createdAt` is kept.
- `uploadImage` keeps an image (1 MB at most) as the group's blob, for
  the profile to reference: a blob in anyone else's repo would not
  resolve for it.
- `putRule` makes or changes a rule. A rule keeps its key when changed,
  so a label citing it still does; a new one goes last unless it says
  where. `deleteRule` takes one away; a label citing it keeps the URI.

## Its handle

A group starts with a handle one label under `HANDLE_DOMAIN`, which is
also its address, kept apart from its handle: where its site is served,
which never changes here. It may take a handle on a domain of its own
instead, once the domain names it, as any handle is resolved
(`HANDLE_RESOLVERS`: the domain's `/.well-known/atproto-did`, then a
service answering `resolveHandle`, which reads its `_atproto` DNS
record). Under `HANDLE_DOMAIN` it may take only its own address back, so
no group takes another's, and going back is always possible.

- `checkGroupHandle` says whether a domain is ready, and whose it names
  now. It changes nothing.
- `updateGroupHandle` takes it: the group's `did:plc` first, signed with
  the host's rotation key (refused, `NotController`, once the host holds
  none for it), then the host, then the relays with an `#identity`
  event.

Both are for a caller holding `group.configure`.

## Switching off, on, and deleting

- `deactivateGroup` switches a group off, for one holding
  `group.configure`. Its account is deactivated, as a PDS deactivates
  one, and the relays are told: its repo and its repos in spaces read as
  `RepoDeactivated`, nothing is written as it, and its spaces issue no
  credentials and admit no writes, so nobody, its admins included, reads
  them. `getGroup` and `listGroups` show it as switched off and nothing
  more. Since its memberships can't be read while it is off, the host
  keeps who held `group.configure` then.
- `activateGroup` switches it back on, for one of those.
- `deleteGroup` deletes it, for one holding `group.configure` (or, while
  it is off, one who did then): the services syncing its spaces are told
  (`notifySpaceDeleted`, best effort, signed by the group while it still
  has a key), its account, repos, spaces and blobs go, the relays are
  told, and its DID is ended in the directory with a tombstone, as only
  this host holds a key to it. What members wrote in its spaces is
  theirs, at their own PDSes, and stays.

A group whose setup fails partway is deleted the same way, so its handle
is free again.

## From the prototype

As `bootstrap.ts` and `methods/groups.ts`, with these changes:

- the spaces are the host's own, decided by the access records directly,
  where the prototype gave each a `managedspace` policy naming the host
  and asked itself;
- lists in records are of objects (`[{role}]`, `[{scope}]`), as this
  host's lexicons have them;
- `createGroup`'s `joinPolicy` takes the profile's words (`approval`) or
  the method's (`fyi.opensocial.profile#request`);
- a group's address is its handle, until groups take handles of their
  own;
- `getGroup` names a member by DID alone, and has no join requests yet:
  people's profiles and joining arrive with membership.

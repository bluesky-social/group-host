# Signing in

A group is an atproto account with no password and no single owner. But
apps still need to sign in as it, to write its records and act in its
spaces. Whatever an app gets must never exceed what the member who let
it in may do, both when they approve it and at every write after. The
host signs people, apps and MCP clients in, and keeps each token inside
its member's roles. Roles are in [membership](membership.md), spaces in
[spaces](spaces.md), and exact methods and parameters in
[the API reference](../reference/api.md).

## Who signs in to what

| Who                      | Signs in as | How                                    | Holds                   |
| ------------------------ | ----------- | -------------------------------------- | ----------------------- |
| A person, in the console | themselves  | OAuth at their own PDS                 | a session cookie        |
| An app                   | a group     | OAuth at this host, or an app password | a group grant           |
| The console              | a group     | `fyi.opensocial.getGroupAuth`          | a 15-minute group token |
| An MCP client            | a member    | OAuth at `/mcp`                        | an MCP grant            |

Members keep their accounts on their own PDSes, so the host only needs
to learn who a person is. A group's repo is here, so this host is its
PDS. It's also its OAuth authorization server, the server an app signs
in at.

An MCP client is an AI app speaking MCP, and it acts as a member. It
doesn't act as the group. The host runs a separate OAuth server for it,
with issuer `<PUBLIC_URL>/mcp`. Its Bearer tokens are good only at
`/mcp`, and the host keeps them apart from group grants. A member
approves an MCP client on the same console page as an app. [MCP](mcp.md)
covers the rest.

## Signing people in to the console

The console's methods act as the group, so the host has no use for a
person's own tokens. It needs to know who's at the browser, and it keeps
nothing else.

The host is a confidential OAuth client of each person's PDS, with
client id `<CONSOLE_URL>/console/client-metadata.json`. Its client key
is derived from the host's rotation secret, so it needs no storage. The
sign-in asks for `atproto` alone, which proves the account and grants
nothing.

If you aren't signed in, you see no group, since the host tells you
nothing about one but its DID and handle ([groups](groups.md#what-each-person-sees-and-finding-a-group)).
The console's home page names the host and asks you to sign in. A
group's page shows its handle and asks the same. Either one opens the
sign-in dialog, which comes back to the page it was opened on.

You type your handle or DID. The host finds your PDS and its
authorization server, pushes the request with PKCE and a fresh DPoP key,
and sends the browser there. On return it exchanges the code, checks
that the token names the DID that started, and revokes the access token
at once.

If you have no account, you follow "Create one". That makes one at the
host's `SIGNUP_PDS` (Bluesky's, unless set) and signs you in with it.
The host pushes a request to that PDS's authorization server the same
way as for a sign-in, with `prompt=create` and no handle, and the server
shows its sign-up. On return the token's `sub` is the account it made,
which nobody named before. So the host resolves it, as it would a typed
handle, and signs you in only if that account's own authorization server
is the one that answered. The sign-in field's example handle is under
the sign-up PDS's own name, `you.bsky.social` or
`you.pds.next.opensocial.fyi`.

As you type, the field suggests the accounts whose handle matches, with
each one's picture, name and handle where they're known. Picking one
signs in as it at once. From two letters on, the browser asks the host
(`fyi.opensocial.searchActors`), and the host asks wherever
`ACTOR_SEARCH` says. On the network everyone's on, that's
typeahead.waow.tech, a service that knows the accounts of the whole
network. So what you type reaches it from the host and never from a
browser. On a network of its own, whose PLC directory isn't
plc.directory, the host reads the handles that directory names from its
export and keeps them a minute. It suggests those by handle alone, since
a directory has no names or pictures. The console's scripts reach no
server but the host, as its content security policy says. When the host
can't suggest anything, the field is a plain one.

Every other field for someone's handle suggests accounts the same way,
and a pick only fills it in. That covers inviting someone, labelling
someone, and the account to move here. The handle is resolved when the
form is sent, so an invite is still written to the account's DID.

The host keeps a cookie, `group_host_session`, holding the DID and its
times. It's signed with an HMAC under another key derived from the
rotation secret, and it's good for 30 days. Signing out records the time
in `console_logout`. A cookie signed before a person's last sign-out
names no one, in any browser.

The cookie is HttpOnly and `SameSite=Lax`. Where the console is on https
(`CONSOLE_URL`), it's also Secure and named `__Host-group_host_session`,
and so are the cookies beside it. A browser then keeps it for the
console's name alone, for its whole path. So a page at a name beside the
console's, such as a group's address, can't set one in its place. A
request whose `Origin` isn't the console's names no one, whatever cookie
it carries. From such a page, the host refuses signing in to another
account (`/console/switch`) or out (`/console/logout`) with 403. The
host answers as usual for a page of the console's own, or a request that
sends no `Origin` (as a browser's own navigations and the console's
server do).

Details:

- The host keeps the sign-in under way by its `state` for 15 minutes,
  and the browser holds the same `state` in a cookie. The callback
  requires both.
- The authorization server's `issuer` must equal the URL its metadata
  came from. The callback must carry `iss`, equal to that issuer.
  Atproto's servers always send it, and the host refuses a callback
  without it.
- The host fetches every server it reaches only at a public address, the
  same as client metadata (below). It keeps `back` only as a path on the
  console.

Moving an account onto this host needs more, because the host must act
at the old PDS. That sign-in asks a wider scope, needs a console session
already, and keeps its tokens at most 24 hours. [moving](moving.md)
explains it.

## Calling the host's own methods

A caller asks the `fyi.opensocial.*` methods as a person. The caller
sends service auth, a short-lived JWT their own PDS signs with `aud` the
host's `SERVICE_DID` and `lxm` the method called. With no
`Authorization` header at all, the console's cookie names the caller
instead. The host's DID document names a `#opensocial` service, so a PDS
can proxy these calls.

Writing a group's records is different. Those calls go to the group's
PDS, which is this host, with a token issued as the group. The rest of
this page is about those tokens.

## Signing an app in as a group

A group has no password to type, so a member signs an app in for it. The
app asks this host, and a member approves in the console and chooses
which of their groups it acts as.

An app finds this host the usual atproto way. The group's DID document
names this host as its PDS, and the host's
`/.well-known/oauth-protected-resource` names `PUBLIC_URL` as its sole
authorization server. The flow is atproto OAuth. The app pushes its
request to `/oauth/par` with PKCE and a DPoP proof, asking for `atproto`
and only scopes its client metadata declares. `/oauth/authorize`
forwards the browser to the console's approval page. After approval the
app exchanges the code at `/oauth/token` for an access and a refresh
token. Both are bound to its DPoP key, with `sub` the group's DID.

A client is the https URL of its metadata document, or the loopback
client `http://localhost` for programs on the same machine. The host
fetches a document only at a public address, and pins the connection to
the address it checked. So a client id can't make the host reach into
its own network. It keeps a document ten minutes.

A public client holds no secret. A confidential one signs a client
assertion with a key its metadata lists, or one at its `jwks_uri`. The
host keeps a key set from a `jwks_uri` ten minutes too. When an
assertion names a key it doesn't hold, it fetches the set again once, so
keys can rotate. A confidential client's grant lasts 90 days
unrefreshed and a public one's lasts 14, since nothing but its DPoP key
ties a public client to its grant. Either one must keep authenticating
the same way, with the same key, for as long as the grant lasts. A
native application must be public.

Details:

- A redirect is registered if it's listed exactly, or, for an http
  loopback address, listed without its port.
- When `PUBLIC_URL` is http, the host allows client documents at any
  address, for test networks.
- DPoP proofs are ES256 or ES256K, and each must carry a nonce from the
  host (`use_dpop_nonce` otherwise).

## Approving

The approval page asks `getAuthorization` what the request is: the
client, the scopes asked, and the groups offered. If the app's
`login_hint` names a switched-on group here, that group is offered.
Otherwise the page offers every switched-on group where the member holds
a role. For each group, the page shows what the app would get and what
it wouldn't, the way an account's PDS shows a request on its consent
page. It reads the scopes with the same parser (`@atproto/oauth-scopes`)
and lists them as that page's rows, in its order (Bluesky, chat, each
permission set, the repository, calling services, spaces). Each row's
details are a dialog away, and the scopes as asked sit behind "Technical
details". Each of the host's permission sets is named in its own words,
the `title` and `detail` of its lexicon, both those asked for and any
tier a group would give in place of one.

`approveAuthorization` requires the member to hold a role in the chosen
group. If the app named a group, the chosen group must be that one. The
host narrows the scope to the member's roles, issues a code good for 60
seconds, and returns the redirect. `declineAuthorization` drops the
request and sends back `access_denied`. Both take service auth or the
cookie.

The grant records the member who approved it, so the host can ask their
roles again later. It doesn't record one of their roles.

## What a token may do

At approval, the host narrows the asked scope to what the roles grant
then. At every write, it checks both that scope and the roles as they
are at that moment.

### Narrowing at approval

- `atproto` is always kept.
- The host's permission sets for a group's session are three tiers, each
  holding the one before it:

  | Set                                        | Title                | Covers                                                                                                                       |
  | ------------------------------------------ | -------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
  | `include:fyi.opensocial.authManageProfile` | Group profile        | the profile and rules in the meta space                                                                                      |
  | `include:fyi.opensocial.authModerate`      | Group moderation     | memberships and labels in the members space, labels in the meta space, invites in people's inboxes, join requests, the queue |
  | `include:fyi.opensocial.authManageGroup`   | Group administration | all of that, the labeler, roles, and changing the meta and members spaces (`manage=update`)                                  |

  A set is an upper bound: of one asked for, the token keeps the parts
  the roles grant. Of each space permission, it keeps the collections
  the roles write there as the group, with the writes they may make (one
  scope for each set of writes). It keeps `manage=update` where the roles
  may change that space, the invites where they hold `invite`, and each
  method where they hold the action that method asks: `admit` for
  `listJoinRequests` and `denyJoinRequest`, `invite` for `listInvites`,
  `mod.read` for `listModSubjects`, `getModSubjectHistory` and
  `getModSubjectBlob`, `mod.resolve` for `resolveModSubject`. When
  what's kept is the whole of a tier, it's kept as that set. So a
  moderator asked for
  `authManageGroup` gives `authModerate`, and a member whose role writes
  the profile and rules by the meta space's policy gives
  `authManageProfile`. Otherwise the parts are kept as scopes.

  `include:fyi.opensocial.authParticipate` is for a person's own
  session, at their PDS, and it's never kept for a group's.

- `repo:C` is kept if the roles' grants hold `repo:C` or `repo:*`.
- `rpc:M` is kept only if the roles hold `repo:*` and `M` is neither
  `*` nor a `fyi.opensocial.*` method.
- `space:T?…` is kept if some space of type `T` lets the roles write
  what's asked there. For a scope that only reads, reading it there is
  enough. In the members space, the roles write its roles if they hold
  `group.configure`. They write memberships as they hold `admit`
  (create), `role.assign` (update) and `eject` (delete).
- `space:fyi.opensocial.invites?…` (people's inboxes) is kept if the
  roles hold `invite` and it asks for invites alone, in anyone's inbox or
  one person's.
- `space:*?…` becomes one scope per space the roles may read, naming the
  asked collections they may write.
- The roles write labels in a space where their scopes cover writing
  `com.atproto.label.record` there: the moderation sets' in the meta and
  members spaces, `space:*?collection=com.atproto.label.record` or a
  scope naming every collection in every space, and a scope the space's
  policy grants them in that space ([moderation](moderation.md#labels)).
  So a role holding only `authModerate` keeps no labels in any other
  space.
- `space:T?…&manage=…` is kept if the roles may do each operation asked
  (`create`, `update`, `delete`) to the group's spaces of type `T`, or of
  every type for `*`. They may by holding `space.create`,
  `space.configure` or `space.delete`, or, for `update` and `delete`, by
  a `manage=` scope a space's policy grants them there.
- `blob:…` is kept only if something kept writes.

The host withholds everything else: other `include:` sets,
`transition:*`, `identity:` and `account:` scopes, and space scopes for
another authority. It then makes the kept scope concrete. Each
permission set becomes the scopes its lexicon lists, compiled into the
host. That's a space scope for each space permission, or two where it
both writes records and changes the space (one writing, one reading and
changing), and an `rpc:` scope for each method. `authority=self` becomes
the group's DID.

A token calls none of the host's own methods, since those take a
member's service auth. Of the sets, a token makes their writes in the
meta and members spaces and in people's inboxes. Their `rpc:` and
`manage=` scopes say what an app was given, as the opensocial proposal
lists them. A grant keeps this scope through every refresh. Refreshing
doesn't ask the roles again, because each write does.

### Checking each write

An app sends `Authorization: DPoP <token>` with a proof signed by the
key the grant is bound to. A token writes only its own group's repo. It
uses the `com.atproto.repo.*` writes, the same writes in a space
(`com.atproto.space.*`), and `uploadBlob`.

The host checks each record write twice. The token's scope must allow
it, or it fails `ScopeMissing`. The member's roles must allow it at that
moment, or it fails 403 "may no longer write … as the group". What the
roles need depends on where the write goes:

- For a repo write, a repo grant for the collection, or the right to
  write it as the group in one of its open spaces.
- In the meta space, what its policy grants the roles, and every record
  for `group.configure`. Roles in the members space need
  `group.configure`.
- For memberships in the members space, `admit`, `role.assign` or
  `eject`, as the write creates, changes or deletes one, and then what
  the records' rules ask ([membership](membership.md)).
- In someone's inbox, `invite` ([membership](membership.md)).
- For a label, scopes of the roles that write labels in that space, as
  at approval ([moderation](moderation.md#labels)).
- Anywhere else, what the scopes the space's policy grants the roles
  allow. A space's published policy and the members space's index of
  spaces are never written as records: only the space methods change
  them ([spaces](spaces.md#making-changing-and-ending-a-space)).

The host checks both under the group's write lock. The group makes all
its commits under that lock, one at a time, to its public repo and its
repos in spaces. So no change of roles slips in between. The exception
is an invite, whose checks run just before its write. Other groups'
commits run beside them. The host checks an upload the same way. The
scope must take the file's type, and the roles must still let the
member write something as the group.

So if a member's roles shrink, every app they signed in gets less at
once. If a member leaves or is removed, every grant they approved ends.
A group that's switched off refuses every write and upload. But its
tokens still refresh and `getSession` answers `status: "deactivated"`,
so its apps carry on when it's switched on again.

Details:

- `applyWrites` takes at most 200 writes. Repo writes honour
  `swapCommit` and `swapRecord`.
- The meta space takes only a profile, rules and the labeler, and the
  members space only roles and memberships. An inbox takes invites, one
  write at a time.
- The host keeps a write of the profile, a rule or the declaration only
  as the group would keep it. A profile needs a name, a known join
  policy and pictures that are the group's own images, at `self`, and
  it's never deleted. A rule needs a title, at a TID. A declaration must
  name `meta`, and only while anyone may read it ([groups](groups.md#changing-its-profile-rules-and-pictures)).
  This is the only way to change them, since no method of the host's
  does.
- Each use marks the grant used, at most once a minute.

## Lifetimes

| What                           | Lasts               | Then                    |
| ------------------------------ | ------------------- | ----------------------- |
| A pushed request               | 5 minutes           | swept on a later push   |
| A code                         | 60 seconds          | deleted when exchanged  |
| An OAuth access token          | 1 hour              | refreshed               |
| A grant, public client         | 14 days unrefreshed | each refresh extends it |
| A grant, confidential client   | 90 days unrefreshed | each refresh extends it |
| A group token                  | 15 minutes          | never refreshed         |
| An app password's access token | 2 hours             | refreshed               |
| An app password's session      | 90 days unrefreshed | each refresh extends it |

A refresh replaces both tokens atomically. If one token is refreshed
twice, one refresh fails, and the old pair stops working.

## Why tokens are stored and nonces are not

The host must end a grant the moment someone revokes it, and a
self-contained token wouldn't allow that. So tokens are opaque, 32
random bytes, and the host keeps each grant with only the SHA-256 of its
tokens. Revoking deletes the row, and the next call fails.

DPoP nonces don't need that. A nonce is an HMAC of a three-minute time
step under a key derived from the rotation secret. The host accepts the
current step and the ones either side, so it checks a nonce without
storing one.

## Group tokens for the console

The console acts as a group too. It doesn't run OAuth against the host
it's part of. Instead it asks `fyi.opensocial.getGroupAuth` for a group
and one of its spaces, as the member, with a DPoP proof. For each
collection the member's roles may write in that space as the group, by
the scopes its policy grants them, the token's scope holds a space scope
and a repo scope, plus blob uploads. It also holds the `manage=`
operations the member may use on that space: `update` and `delete` by
their roles or the policy's grants, or `create` for a space not made
yet, if their roles hold `space.create`. If they may do nothing there,
the request fails 403.

For the members space, the token also writes the records there that the
member's grants allow (roles, and memberships as above). In any space,
it writes labels where the member's roles' scopes write them, as at
approval. Each gets a
space scope and no repo scope. That's how the
console creates roles, approves requests, gives roles and removes
members. For someone's inbox, a member who holds `invite` gets a scope
for writing invites there and nothing else. That's how the console
invites someone and cancels an invite.

The token lasts 15 minutes. It's an ordinary grant with source
`getGroupAuth`, checked at each write as above. The console's settings
take one for `meta` to write the group's profile, pictures and rules.
The console ends each token at `/oauth/revoke` once its writes are
made, so none is left among the group's sessions.

An MCP client writes the profile and rules the same way, inside the
host. Each such tool call runs as a grant for its member, with the scope
`getGroupAuth` would give them for `meta` at that moment, and the host
checks it as it checks any write ([MCP](../reference/mcp.md)).

## Listing and revoking

Every grant records its member, its client, its source (`oauth`,
`getGroupAuth`, `appPassword` or `provisionGroup`), its scope, when it
was made, and when it was last used. Members who hold `group.configure`
list every grant in force with `fyi.opensocial.listGroupTokens` and end
any one with `fyi.opensocial.revokeGroupToken`. An app ends its own at
`/oauth/revoke`. MCP grants are listed separately, with
`listConnectedApps`.

`listGroupTokens` also returns the group's latest 20 log entries, each
naming the member, the action, the time and the client. The log records
sign-ins, group tokens taken, each write made with a token, app
passwords made and revoked, and grants revoked with `revokeGroupToken`.
It doesn't record refreshes. It also doesn't record grants that end
another way, at `/oauth/revoke`, by `deleteSession`, or when a member is
removed.

## App passwords

Some apps sign in with a handle and a password through
`com.atproto.server.createSession`, as the Bluesky app does. App
passwords let them sign in as a group.

A member whose roles hold `repo:*` makes one in the console with
`fyi.opensocial.createAppPassword`, and it stays theirs. The password
appears in that answer only, and the host keeps its SHA-256. A group
holds at most 25. Makers list their own with `listAppPasswords`, and
configurers list every one. `revokeAppPassword` deletes one along with
every session signed in with it.

A session is a grant with source `appPassword`. Its tokens are Bearer
JWTs that aren't DPoP-bound, and the host finds them by hash like any
other. Its scope is `atproto transition:generic`, which covers the
group's repo and blobs and never a space. A privileged password adds
`transition:chat.bsky`, though the host serves no chat.

The host can't narrow its scope, so it follows its maker's roles more
bluntly. At each use and refresh the host checks that the password still
exists. At each sign-in, use and refresh, and whenever the group's
passwords are listed or one is made, it checks that each maker still
holds `repo:*`. Once a maker doesn't, the host forgets every app
password of theirs in the group and ends every session from one. So
none of them works again even if the maker gets `repo:*` back.

## Provisioning

An app that starts groups wants to sign in to the new group in the same
step. `fyi.opensocial.provisionGroup` does both. The person calls it as
themselves and becomes the founder. The app proves itself in the same
call with its `clientId`, a client assertion if it's confidential, a
DPoP proof, and the scope it wants. That scope must hold `atproto` and
only scopes its metadata declares. Only an app listed in
`PROVISIONING_CLIENTS` ([configuration](../reference/config.md)) may do
this.

The host checks all of that before it makes the group, with
`createGroup`'s fields ([groups](groups.md)). It then narrows the scope
to the founder's roles, as approval would, and answers with the group
and a token response. The session is an ordinary grant with source
`provisionGroup`. It lasts as one from a code would, refreshes at
`/oauth/token`, is listed, and can be revoked.

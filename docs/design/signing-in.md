# Signing in

A group is an atproto account with no password and no single owner. Apps
still need to sign in as it, to write its records and act in its spaces.
Whatever an app gets must never exceed what the member who let it in may
do, both when they approve it and at every write after. This page
explains how the host signs people, apps and assistants in, and how it
keeps each token inside its member's roles. Roles are in
[membership](membership.md), spaces in [spaces](spaces.md), and exact
methods and parameters in [the API reference](../reference/api.md).

## Who signs in to what

| Who | Signs in as | How | Holds |
| --- | --- | --- | --- |
| A person, in the console | themselves | OAuth at their own PDS | a session cookie |
| An app | a group | OAuth at this host, or an app password | a group grant |
| The console | a group | `fyi.opensocial.getGroupAuth` | a 15-minute group token |
| An assistant | a member | OAuth at `/mcp` | an assistant grant |

Members keep their accounts on their own PDSes, so the host only needs
to learn who a person is. A group's repo is here, so this host is its
PDS and its OAuth authorization server, the server an app signs in at.

An assistant, an AI client speaking MCP, acts as a member, not as the
group. The host runs a separate OAuth server for it, with issuer
`<PUBLIC_URL>/mcp`, whose Bearer tokens are good only at `/mcp` and are
kept apart from group grants. A member approves an assistant on the same
console page as an app. [assistants](assistants.md) covers the rest.

## Signing people in to the console

The console's methods act as the group, so the host has no use for a
person's own tokens. It needs to know who is at the browser, and keeps
nothing else.

The host is a confidential OAuth client of each person's PDS, with
client id `<CONSOLE_URL>/console/client-metadata.json`. Its client key
is derived from the host's rotation secret, so it needs no storage. The
sign-in asks for `atproto` alone, which proves the account and grants
nothing.

Someone not signed in is shown no group, since the host tells them
nothing of one but its DID and handle ([groups](groups.md#what-each-person-sees-and-finding-a-group)).
The console's home page names the host and asks them to sign in; a
group's page shows its handle and asks the same. Either opens the
sign-in dialog, which comes back to the page it was opened on.

A person types their handle or DID. The host finds their PDS and its
authorization server, pushes the request with PKCE and a fresh DPoP key,
and sends the browser there. On return it exchanges the code, checks the
token names the DID that started, and revokes the access token at once.

What it keeps is a cookie, `group_host_session`: the DID and its times,
signed with an HMAC under another key derived from the rotation secret,
good for 30 days. Signing out records the time in `console_logout`, and
a cookie signed before a person's last sign-out names no one, in any
browser.

Details:

- The sign-in under way is kept by its `state` for 15 minutes, and the
  browser holds the same `state` in a cookie; the callback requires
  both.
- The authorization server's `issuer` must equal the URL its metadata
  came from, and the callback must carry `iss`, equal to that issuer:
  atproto's servers always send it, and a callback without it is
  refused.
- Every server reached is fetched only at a public address, as client
  metadata is (below). `back` is kept only as a path on the console.

Moving an account onto this host needs more: the host must act at the
old PDS. That sign-in asks a wider scope, needs a console session
already, and keeps its tokens at most 24 hours. [moving](moving.md)
explains it.

## Calling the host's own methods

The `fyi.opensocial.*` methods are asked as a person. A caller sends
service auth, a short-lived JWT their own PDS signs with `aud` the
host's `SERVICE_DID` and `lxm` the method called. With no
`Authorization` header at all, the console's cookie names the caller
instead. The host's DID document names a `#opensocial` service, so a PDS
can proxy these calls.

Writing a group's records is different. Those calls go to the group's
PDS, which is this host, with a token issued as the group. The rest of
this page is about those tokens.

## Signing an app in as a group

A group has no password to type, so a member signs an app in for it. The
app asks this host, and a member approves in the console, choosing which
of their groups it acts as.

An app finds this host the usual atproto way: the group's DID document
names this host as its PDS, whose
`/.well-known/oauth-protected-resource` names `PUBLIC_URL` as its sole
authorization server. The flow is atproto OAuth. The app pushes its
request to `/oauth/par` with PKCE and a DPoP proof, asking for `atproto`
and only scopes its client metadata declares. `/oauth/authorize`
forwards the browser to the console's approval page. After approval the
app exchanges the code at `/oauth/token` for an access and a refresh
token, both bound to its DPoP key, with `sub` the group's DID.

A client is the https URL of its metadata document, or the loopback
client `http://localhost` for programs on the same machine. The host
fetches a document only at a public address, pinning the connection to
the address it checked, so a client id cannot make the host reach into
its own network. It keeps a document ten minutes.

A public client holds no secret; a confidential one signs a client
assertion with a key its metadata lists, or one at its `jwks_uri`. The
host keeps a key set from a `jwks_uri` ten minutes too, and fetches it
again once when an assertion names a key it does not hold, so keys can
rotate. A confidential client's grant
lasts 90 days unrefreshed and a public one's 14, since nothing but its
DPoP key ties a public client to its grant. Either must keep
authenticating the same way, with the same key, for as long as the grant
lasts. A native application must be public.

Details:

- A redirect is registered if listed exactly, or, for an http loopback
  address, listed without its port.
- When `PUBLIC_URL` is http, client documents at any address are
  allowed, for test networks.
- DPoP proofs are ES256 or ES256K, and each must carry a nonce from the
  host (`use_dpop_nonce` otherwise).

## Approving

The approval page asks `getAuthorization` what the request is: the
client, the scopes asked, and the groups offered. If the app's
`login_hint` names a switched-on group here, that group is offered;
otherwise every switched-on group where the member holds a role. For
each, the page shows the member's roles and which asked scopes they
would grant or withhold, and each of the host's permission sets asked
for in its own words, the `detail` of its lexicon.

`approveAuthorization` requires the member to hold a role in the chosen
group, and that group to be the one the app named, if it named one. The
host narrows the scope to the member's roles, issues a code good for 60
seconds, and returns the redirect. `declineAuthorization` drops the
request and sends back `access_denied`. Both take service auth or the
cookie.

The grant records the member who approved it, not one of their roles, so
the host can ask their roles again later.

## What a token may do

At approval, the asked scope is narrowed to what the roles grant then.
At every write, both that scope and the roles as they are now are asked.

### Narrowing at approval

- `atproto` is always kept.
- One of the host's permission sets for a group's session is kept if
  the roles hold every action it covers:

  | Set | Kept for roles holding |
  | --- | --- |
  | `include:fyi.opensocial.configure` | `group.configure` |
  | `include:fyi.opensocial.manageMembers` | `admit`, `eject`, `role.assign` and `invite` |
  | `include:fyi.opensocial.manageSpaces` | `space.configure` |
  | `include:fyi.opensocial.moderate` | `mod.read`, `mod.resolve` and `label` |

  `include:fyi.opensocial.participate` is for a person's own session, at
  their PDS, and never kept for a group's.
- `repo:C` is kept if the roles' grants hold `repo:C` or `repo:*`.
- `rpc:M` is kept only if the roles hold `repo:*`, and `M` is neither
  `*` nor a `fyi.opensocial.*` method.
- `space:T?…` is kept if some space of type `T` lets the roles write
  what is asked there, or read it, for a scope that only reads.
- `space:fyi.opensocial.invites?…`, people's inboxes, is kept if the
  roles hold `invite` and it asks for invites alone, in anyone's inbox or
  one person's.
- `space:*?…` becomes one scope per space the roles may read, naming the
  asked collections they may write.
- `blob:…` is kept only if something kept writes.

Everything else is withheld: other `include:` sets, `transition:*`,
`identity:` and `account:` scopes, and space scopes for another
authority. The kept scope is then made concrete. Each permission set
becomes the scopes its lexicon lists, compiled into the host: a space
scope for each space permission, and an `rpc:` scope for each method.
`authority=self` becomes the group's DID.

A token calls none of the host's own methods, which take a member's
service auth, and writes in the meta and members spaces only what the
configure set is for (below). So of the four sets only configure's
writes, and manageMembers' invites in people's inboxes, can be made with
a token now; the others' scopes say what an app was given, as the
opensocial proposal lists them. A grant keeps this scope
through every refresh; refreshing does not ask the roles again, because
each write does.

### Checking each write

An app sends `Authorization: DPoP <token>` with a proof signed by the
key the grant is bound to. A token writes only its own group's repo,
with the `com.atproto.repo.*` writes, the same writes in a space
(`com.atproto.space.*`), and `uploadBlob`.

Each record write is asked twice. The token's scope must allow it, or it
fails `ScopeMissing`. The member's roles must allow it now, or it fails
403 "may no longer write … as the group". For a repo write the roles
need a repo grant for the collection, or the right to write it as the
group in one of its open spaces. For a space write they need
`group.configure` in the meta and members spaces; in someone's inbox,
`invite` ([membership](membership.md)); in a space for chosen people, to
be one of them and hold `repo:*`; and otherwise what the space's access
record grants. Both are asked under the lock the commit is made under,
so no change of roles slips in between, but for an invite, whose checks
are made just before its write. An upload is
asked the same way: the scope must take the file's type, and the roles
must still let the member write something as the group.

A member whose roles shrink therefore gives every app they signed in
less at once, and a member who leaves or is put out ends every grant
they approved. A group switched off refuses every write and upload, but its tokens
still refresh and `getSession` answers `status: "deactivated"`, so its
apps carry on when it is switched on again.

Details:

- `applyWrites` takes at most 200 writes. Repo writes honour
  `swapCommit` and `swapRecord`.
- The meta space takes only a profile, rules and the labeler, and the
  members space only roles. An inbox takes invites, one write at a time. A role still named by a grant cannot be
  deleted.
- A write of the profile, a rule or the declaration is kept only as the
  group would keep it: a profile with a name, a known join policy and
  pictures that are the group's own images, at `self` and never deleted;
  a rule with a title, at a TID; a declaration naming `meta`, only while
  anyone may read it ([groups](groups.md#changing-its-profile-rules-and-pictures)).
  This is the only way to change them; no method of the host's does.
- Each use marks the grant used, at most once a minute.

## Lifetimes

| What | Lasts | Then |
| --- | --- | --- |
| A pushed request | 5 minutes | swept on a later push |
| A code | 60 seconds | deleted when exchanged |
| An OAuth access token | 1 hour | refreshed |
| A grant, public client | 14 days unrefreshed | each refresh extends it |
| A grant, confidential client | 90 days unrefreshed | each refresh extends it |
| A group token | 15 minutes | never refreshed |
| An app password's access token | 2 hours | refreshed |
| An app password's session | 90 days unrefreshed | each refresh extends it |

A refresh replaces both tokens atomically: of two refreshes of one
token, one fails, and the old pair stops working.

## Why tokens are stored and nonces are not

The host must end a grant the moment someone revokes it, which a
self-contained token would not allow. So tokens are opaque, 32 random
bytes, and the host keeps each grant with only the SHA-256 of its
tokens. Revoking deletes the row, and the next call fails.

DPoP nonces need no such power. A nonce is an HMAC of a three-minute
time step under a key derived from the rotation secret. The host accepts
the current step and those either side, so it checks a nonce without
storing one.

## Group tokens for the console

The console acts as a group too. Rather than run OAuth against the host
it is part of, it asks `fyi.opensocial.getGroupAuth` for a group and one
of its spaces, as the member, with a DPoP proof. The token's scope is,
for each collection the member's roles may write in that space, a space
scope and a repo scope, plus blob uploads. In a space for chosen people,
that is every collection for those of them who hold `repo:*`, and
nothing for anyone else. If they write nothing there, the request fails
403. For someone's inbox instead, a member who holds `invite` gets a
scope for writing invites there and nothing else, which is how the
console invites someone and takes an invite back. The token lasts 15
minutes and is an ordinary grant with source `getGroupAuth`, checked at
each write as above. The console's settings take one for `meta` to
write the group's profile, pictures and rules. The console ends each
token at `/oauth/revoke` once its writes are made, so none is left
among the group's sessions.

An assistant connected over MCP writes the profile and rules the same
way, inside the host: each such tool call is made as a grant for its
member, with the scope `getGroupAuth` would give them for `meta` at that
moment, and checked as any write is ([MCP](../reference/mcp.md)).

## Listing and revoking

Every grant records its member, its client, its source (`oauth`,
`getGroupAuth`, `appPassword` or `provisionGroup`), its scope, when it
was made, and when it was last used. Members who hold `group.configure`
list every grant in force with `fyi.opensocial.listGroupTokens` and end
any one with `fyi.opensocial.revokeGroupToken`. An app ends its own at
`/oauth/revoke`. Assistant grants are listed apart, with
`listConnectedApps`.

`listGroupTokens` also returns the group's latest 20 log entries, each
naming the member, the action, the time and the client. The log records
sign-ins, group tokens taken, each write made with a token, app
passwords made and revoked, and grants revoked with `revokeGroupToken`.
It does not record refreshes, or grants that end another way: at
`/oauth/revoke`, by `deleteSession`, or when a member is put out.

## App passwords

Some apps sign in with a handle and a password through
`com.atproto.server.createSession`, as the Bluesky app does. App
passwords let them sign in as a group.

A member whose roles hold `repo:*` makes one in the console with
`fyi.opensocial.createAppPassword`, and it stays theirs. The password
appears in that answer only; the host keeps its SHA-256. A group holds
at most 25. Makers list their own with `listAppPasswords` and
configurers list every one; `revokeAppPassword` deletes one with every
session signed in with it.

A session is a grant with source `appPassword`. Its tokens are Bearer
JWTs, not DPoP-bound, found by hash like any other. Its scope is
`atproto transition:generic`: the group's repo and blobs, never a space.
A privileged password adds `transition:chat.bsky`, though the host
serves no chat.

Its scope cannot be narrowed, so it follows its maker's roles more
bluntly. At each use and refresh the host asks whether the password
still exists. At each sign-in, use and refresh, and whenever the group's
passwords are listed or one is made, it asks whether each maker still
holds `repo:*`. Once one does not, every app password of theirs in
the group is forgotten and every session from one ends, so none works
again if they are given `repo:*` back.

## Provisioning

An app that starts groups wants to sign in to the new group in the same
step. `fyi.opensocial.provisionGroup` does both. The person calls it as
themselves and becomes the founder. The app proves itself in the same
call with its `clientId`, a client assertion if confidential, a DPoP
proof, and the scope it wants, which must hold `atproto` and only scopes
its metadata declares. Only an app listed in `PROVISIONING_CLIENTS`
([configuration](../reference/config.md)) may do this.

The host checks all of that before it makes the group, with
`createGroup`'s fields ([groups](groups.md)). It then narrows the scope
to the founder's roles, as approval would, and answers with the group
and a token response. The session is an ordinary grant with source
`provisionGroup`: it lasts as one from a code, refreshes at
`/oauth/token`, is listed, and can be revoked.

# Signing in

An app signs in to a group as it signs in to any account: with atproto
OAuth, at the group's PDS, which is this host. What is different is who
says yes. A group has no password, so whoever signs an app in as a group
is one of its members, acting with their roles, and the app gets what
those roles allow and nothing more.

## The server

The host is a standard atproto authorization server (`oauth.rs`):

- `/.well-known/oauth-authorization-server` and
  `/.well-known/oauth-protected-resource` say what it is.
- A client pushes its request to `/oauth/par` first (pushed requests are
  required), with PKCE (`S256`) and a DPoP proof, and gets a
  `request_uri` good for five minutes.
- `/oauth/authorize` sends the browser on to the console's `/authorize`
  (`CONSOLE_URL`), where the member approves.
- `/oauth/token` exchanges the code, once, within a minute, for an
  access token (an hour) and a refresh token, both bound to the client's
  DPoP key. A refresh gives a new pair and ends the old one. A grant
  lasts two weeks without a refresh for a public client, three months
  for a confidential one.
- `/oauth/revoke` ends a grant by either of its tokens.

Every endpoint answers any origin, as an app in a browser reaches them.

Clients are known by their metadata documents, or are the loopback
client `http://localhost`, a native app on the member's own machine,
whose redirects are at a loopback address on any port
(`oauth_clients.rs`). A document, and a confidential client's key set, is
fetched over https from a public address only (never loopback, private,
link-local or reserved), the connection pinned to the address checked,
with no redirects, at most 64 KiB, and kept ten minutes. A confidential
client signs each request with a key its metadata lists
(`private_key_jwt`), and must keep signing with the same key, known by
its thumbprint, whatever `kid` it names.

DPoP proofs are ES256 or ES256K, the curves atproto keys are on
(`oauth_dpop.rs`). A nonce is an HMAC of the time, under a key derived
from the host's secret for nonces alone, good for its three
minutes and those either side, so the host checks one without keeping it;
a proof's `jti`, like a client assertion's, is spent once. The JWKs,
thumbprints and JWTs are `atproto::jose`, checked against what jose, the
reference provider's library, makes of the same keys.

A code is exchanged with its PKCE verifier, 43 to 128 unreserved
characters as RFC 7636 has it. Tokens are opaque, kept as their SHA-256, never as themselves, so a
grant ends the moment it is revoked, or its member is put out of the
group (`oauth_store.rs`).

## Approving

The console's approval page asks with the member's own service auth
(`authorization.rs`):

- `getAuthorization` names the app and offers the groups the member may
  sign it in as: the one the app named in its `login_hint`, by handle or
  DID, or every group here, switched on, where they hold a role. For each,
  what the app would get and what it would not.
- `approveAuthorization` signs the app in as the group the member picks.
  They must hold a role there, and it must be the group the app named, if
  it named one. The grant is theirs.
- `declineAuthorization` sends the browser back with `access_denied`.

## What a token may do

The scope an app asks for is narrowed to the member's roles
(`oauth_scope.rs`), each scope kept only if:

- `atproto`: always;
- `include:fyi.opensocial.configure`: the roles configure the group;
- `repo:C`: the roles' grants in the permissions record hold `repo:C`,
  or `repo:*`;
- `rpc:M`, a method of another service: the roles hold `repo:*`, and it
  is not one of this host's own methods, which a member calls as
  themselves;
- `space:T?…`: some space of type `T` of the group's lets the roles
  write as the group each collection asked for, as asked (its access
  record's `credentialScopes`), or read it, if reading is all it asks;
- `space:*?…`: made into one scope for each of the group's spaces the
  roles read, writing there what they may of what it asks;
- `blob:`: beside any of the above that writes.

Everything else is withheld: other permission sets, and the old
`transition:` scopes.

The scope is then made concrete, so a token never depends on how a scope
is read later: the configure set becomes the space scopes its lexicon
lists, `authority=self` becomes the group, and a space scope naming no
collection names every one, which narrowing let through only for roles
that may write every one.

## Using a token

An app calls the group's PDS methods with `Authorization: DPoP <token>`
and a proof of the request that hashes the token (its `ath`), signed by
the key the token is bound to (`oauth_resource.rs`). A token refused is
401 with a `WWW-Authenticate: DPoP error=…` challenge and the nonce for
the next proof, as RFC 9449 has a resource server answer; an app whose
nonce is stale sends the request again with the one it was given.

With it, an app writes as the group (`token_writes.rs`):

- in its repo: `com.atproto.repo.createRecord`, `putRecord`,
  `deleteRecord` and `applyWrites`, each write as its `repo:` scope
  allows, and `uploadBlob`, a type its `blob:` scope takes;
- in its spaces: `com.atproto.space.createRecord`, `putRecord`,
  `deleteRecord` and `applyWrites`, each as its `space:` scope allows;
- and `com.atproto.server.getSession` says which group it is for, and
  whether it is switched on.

Everything a write asks of the repo is asked as it is written, under the
lock its commit is made under, so no other write comes between
(`repo::resolve`): a repo write's `swapCommit` must be the repo's head,
and a put's or delete's `swapRecord` the record there (null for none),
or it is `InvalidSwap`; a put is a create or an update by what is there
then, and is asked of the token's scope and the member's roles as that;
and a put of what is there already, or a delete of what is not, is
asked as any other, then changes nothing and makes no commit. The scope is read as atproto's permission spec reads one
(`scopes.rs`): a space scope's authority is the group unless it names
one, and one naming no action grants every one but `read_self`.

A grant keeps the scope it was approved with through its refreshes, so
what each write may do is asked again of the member's roles as they are
now, as narrowing asked them: one whose roles shrink gives the app less
at once.

In the group's own meta and members spaces, a token writes only what the
configure set names, and only as the host's methods would: the profile
(with a name, and a join policy it knows) and the labeler, kept at
`self` and never deleted; rules; and roles, one taken away only once no
grant names it. Memberships, permissions, and any space's access record
change only through the host's methods, which check that nobody gives
themselves more than they hold.

## Group tokens

The console acts as the group without OAuth's dance (`group_tokens.rs`):
`getGroupAuth` takes the member's own service auth and a DPoP proof, and
gives a token for the group, good for fifteen minutes and never
refreshed, bound to that proof's key. Its scope is what the member's
roles may write as the group in the one space asked for, by its access
record's `credentialScopes`: those collections there, with those writes,
the same kinds of record in the group's repo (a role may publish what it
writes as the group, as the access lexicon has it), and image files, or
any file for a role that writes any kind. A member whose roles write
nothing there gets no token.

## App passwords

An app that signs in with a handle and a password, as the Bluesky app
does, signs in to a group with an app password (`app_passwords.rs`). A
group has no password, and one does not give it one: a member who may do
anything as the group (every repo collection) makes it in the console
(`createAppPassword`), named for where it is used, and it stays theirs.
It is shown once; the host keeps its SHA-256 alone, as it is 80 random
bits, which no slow hash would make harder to guess. A group holds 25 at
most. `listAppPasswords` shows every one to the group's configurers, and
each maker their own; `revokeAppPassword` takes one away, its maker's or
any for a configurer, and signs out every app signed in with it.

`com.atproto.server.createSession` signs in with one, by the group's
handle or DID; `refreshSession` gives a new pair and ends the old; and
`deleteSession` signs out. A session is a grant like any other, listed
among the group's, its tokens bearer ones and JWTs, as a PDS's are, so an
app may read when they lapse; the host still finds them by their hash. A
lapsed access token is a 400 `ExpiredToken`, the sign to refresh. Its
scope is `atproto transition:generic`, the group's repo and blobs and
never a space, with `transition:chat.bsky` for a privileged one.

An app password works only while its maker could make it: at each use,
sign-in and refresh, the host asks, and once they could not (they lost
the role, or left), every app password of theirs is forgotten and every
session from one signed out.

## Who acts as the group

Every grant names the member who gave it, its app, how it was given
(`oauth`, `getGroupAuth` or `appPassword`), its scope, and when it was last used, to the
minute. `listGroupTokens` shows a group's configurers every grant that
still lasts, and its log: each sign-in, each token taken, each record a
grant wrote, and each grant ended, the latest twenty. `revokeGroupToken`
ends one.

## From the prototype

- Tokens are opaque and kept as hashes; the prototype signed access JWTs
  and kept refresh tokens as they were.
- No lexicon is resolved at sign-in. The prototype resolved permission
  sets and space types; here every permission set but the host's own is
  withheld, as the prototype withheld them too, and the host's own is
  read from its lexicon.
- DPoP proofs and client assertions are ES256 or ES256K, the algorithms
  atproto names, not every algorithm jose knows.
- A grant records the member who approved it, not one of their roles.
- The token and revoke endpoints take a form, as OAuth has them, not
  JSON.
- What a token may write is asked of the member's roles at each use; the
  prototype asked only at approval.
- Records are checked against the data model and their collection's
  `$type`, not against their lexicons, so `validationStatus` is
  `unknown`, as a PDS answers for a lexicon it does not know.

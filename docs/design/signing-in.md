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

A grant keeps the scope it was approved with through its refreshes, so
what it may do is asked again of the member's roles at each use: one
whose roles shrink gives the app less at once. (That is the resource
side, which comes next.)

The scope is then made concrete, so a token never depends on how a scope
is read later: the configure set becomes the space scopes its lexicon
lists, `authority=self` becomes the group, and a space scope naming no
collection names every one, which narrowing let through only for roles
that may write every one.

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

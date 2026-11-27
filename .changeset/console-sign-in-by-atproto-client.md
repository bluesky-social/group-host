---
default: minor
---

# The console signs people in with atproto-client's OAuth client

Signing in to the console, signing up, and signing in to an account to move here go through `atproto-client`'s OAuth client, which holds an authorization server to atproto's profile: it must require PAR, take client metadata documents, say who answers (`iss`, which a callback must carry), and list the methods the profile asks. Before the tokens are taken, at the callback and before each refresh of a move's session, the account's authorization server is read again from its DID and must be the one that issued them. A sign-in refused after its code was redeemed, as another account's, has its grant revoked. What a sign-in gives back is its refresh token, which ends the whole grant, where it was the access token.

The console's client metadata is a confidential web client's as atproto's profile has it, with a second redirect, `/console/move/callback`, for an account being moved here; it answers 500, saying why, where `CONSOLE_URL` is not https at a public name, as no authorization server takes such a client. A sign-in waiting for the browser is kept in the database whole, and a move keeps its account's session there by the account; a move under way when the host is upgraded is let go, to be signed in to again. An account's server is read at most 16 MiB an answer, and 256 MiB a repo or file, while it moves.

Each call the host makes to a host others' data names (sending a call on, telling a PDS or a syncer, reading a client's metadata, a group's own domain) is now logged as a `call` line and sends a `traceparent`, as identity calls were.

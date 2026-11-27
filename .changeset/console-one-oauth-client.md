---
default: patch
---

# The console signs in with one OAuth client, and checks service auth by atproto-client

The console signs people in, and signs in to accounts being moved here, with one OAuth client on atproto-client 0.6. A sign-in coming back to the other one's callback (the console's at a move's, or a move's at the console's) is refused as one this browser did not start, approved or not. A move finished or cancelled while its account's session was being refreshed stays ended: the refreshed tokens are given back, and the move answers `NotFound`.

Service auth sent to the host's own methods, to `notifyRepoUpdate` and `notifyCredentialsRevoked`, and an account's token for moving here, are checked with atproto-client's `verify_any`, for the audiences each took before. A token's audience and method are now checked before any key is read: `notifyRepoUpdate` checks its `space` is well formed first, and a token for an account not hosted here is refused by `notifyCredentialsRevoked` before its signature.
An account's token for moving here whose DID can't be resolved is refused `InvalidToken`, as one whose DID names no key is, where it was `InvalidRequest`.

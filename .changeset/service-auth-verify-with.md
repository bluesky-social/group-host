---
default: patch
---

# Service auth is checked with atproto-client 0.7.1's `verify_with`

The host's service auth and an account's token for moving here are checked with atproto-client 0.7.1's `verify_with`, which judges a token's audience itself, and tells an issuer that can't be resolved from one naming no key. An account's token for moving here whose DID can't be resolved is refused `InvalidRequest` again, as before 0.6; one whose DID names no key, or whose signature doesn't hold, is still `InvalidToken`.

---
default: minor
---

# Identities, guarded requests and service tokens by atproto-client

The host resolves identities, asks the hosts others' data names, and checks a space's delegation tokens and a group's token for moving here with the `atproto-client` crate. Its requests to such hosts now share one pool of connections rather than a client each. Whose a handle is, it keeps five minutes as it keeps DID documents, but asks again to check a group's own domain or an account moving here. `HANDLE_RESOLVERS` names a handle's own `/.well-known/atproto-did` as `well-known`, asked over the scheme `DID_WEB_SCHEME` says, where it was `well-known:<scheme>`: the host refuses to start with the old form. With `DID_WEB_SCHEME=http`, as on a test network, a handle may be any name, as `127.0.0.1:2584`. `atgrp` calls hosts and its own PDS through the same crate.

Each call the host makes to resolve an identity or check a token is logged as a `call` line, attempt by attempt, in the trace of the request it is made for, and sends a `traceparent` naming that trace; a refusal it is answered with is logged as `refused`.

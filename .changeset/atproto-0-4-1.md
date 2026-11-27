---
default: patch
---

# DPoP proofs, PKCE, did:webs and AT URIs are read by atproto 0.4.1

The host checks DPoP proofs, PKCE verifiers and challenges, `did:web`s and AT URIs with the `atproto` crate's own, as the reference implementation does. A DPoP proof with an `ath` sent with no access token, as to `/oauth/par` or `/oauth/token`, is now refused (`invalid_dpop_proof`), where before its `ath` went unread. A pushed authorization request whose `code_challenge` is not 43 to 128 unreserved characters is refused there, rather than at the token endpoint. A `PUBLIC_URL` whose host is no hostname, with a port if any, is refused at start unless `SERVICE_DID` is set, and `atgrp` refuses an `ATGRP_HOST` or `--to` address whose did:web it cannot make, unless `ATGRP_HOST_DID` or `--to-did` names one.

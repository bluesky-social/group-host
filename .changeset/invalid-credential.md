---
default: patch
---

# A space credential for another space is refused as InvalidCredential

A space read or an authority's method called with a sound space credential for another space is refused with 400 `InvalidCredential`, as the spaces alpha's reference refuses it, where it was 401 `BadJwtSub`. It is checked last, after the request's signature, revocation and audience, and a credential is looked up as revoked under the space it was issued for.

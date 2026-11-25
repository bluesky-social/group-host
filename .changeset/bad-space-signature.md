---
default: patch
---

# A space read's signature and audience are refused as the reference refuses them

A space credential's request whose HTTP Message Signature fails, or whose `Atproto-Space-Audience` is missing or not a DID, is refused with 401 `BadSpaceSignature`, the spaces alpha's name, where it was `BadSignature`. A request signed for another repo than the one it reads is refused with 401 `BadSpaceAudience`, where it was 400, and only once its signature holds.

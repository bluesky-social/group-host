---
default: patch
---

# Scopes read as atproto's reference reads them

The host reads OAuth scopes, roles' permissions and space policies' permissions with atproto 0.4.4's `scope` module, as atproto's permission spec and `@atproto/oauth-scopes` read them. A scope its grammar doesn't take is no scope: one naming an action there is none of (`repo:C?action=upsert`, where the unknown action was left out before), a parameter its resource doesn't take, a value it takes once named twice, a collection that isn't an NSID, or an `rpc:` audience with no `#service`. Approval withholds such a scope, and `createSpace` and `updateSpace` refuse a policy permission that is one (`InvalidRequest`).

A blob scope names one type or pattern: `blob:image/*,video/*` is no scope, where it was read as both. An upload's type is compared with the scope as written, not ignoring case, so `IMAGE/png` no longer passes `blob:image/*`. A token now keeps each blob scope it is given as one scope per type, lowercase (`blob?accept=Image/PNG` becomes `blob:image/png`).

An `rpc:` scope for one of the host's own methods names it at `<host DID>#atgrp` or `*`; the host's DID alone, which it took before, is no audience. `space?spaceType=T` is now kept at approval as `space:T` is.

Tokens already issued keep what they were granted: upgrading rewrites each grant's scope once so that it reads the same. A blob list becomes a scope per type, lowercase; unknown actions and parameters, and repeated values after the first, are dropped; and the host's DID alone as an audience of its own methods becomes `<host DID>#atgrp`. A scope that granted nothing before still does. Roles and space policies already written are records and are not rewritten: a permission in one that the grammar doesn't take now grants nothing. Those the console and `atgrp` write all read the same.

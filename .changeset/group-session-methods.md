---
default: minor
---

# An app signed in as a group calls its methods with rpc: scopes, and what its grant bought ends with it

Approval keeps `rpc:org.atgrp.listJoinRequests`, `denyJoinRequest`, `listModSubjects`, `getModSubjectHistory` and `resolveModSubject` (for this host or `aud=*`) when the approving member's roles hold the action each asks (`admit`, `mod.read`, `mod.resolve`). Those methods then answer the group session's DPoP token as well as a member's service auth, asking the member's roles again at each call (`Forbidden`), and the scope (`ScopeMissing`).

The delegation tokens a group session is given, and the space credentials and `notify` registrations they buy, are kept with its grant. When the grant ends (`revokeGroupToken`, `/oauth/revoke`, `deleteSession`, its member put out), or its member reads a space no more after a members-space write or a policy change, its credentials there are revoked, the writers' PDSes told, its registrations withdrawn, and its unspent delegations buy nothing.

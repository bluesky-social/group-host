---
default: minor
---

# /oauth/token names the approving member

A token response, to a code or a refresh, carries `member`, the DID of the member who approved the grant: this host's own field, beside `sub`, the group.

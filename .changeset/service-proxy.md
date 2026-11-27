---
default: minor
---

# An app signed in as a group calls other services through the host

An app signed in as a group (with OAuth or an app password) calls other services as the group the way it would through any account's PDS: a call with `atproto-proxy: <did>#<service>` to a method the host doesn't serve is sent on to that service with service auth the group signs (`aud` the service's DID, `lxm` the method, a minute), and its answer streamed back. `com.atproto.server.getServiceAuth` gives an app such a token to send itself. A token's scope must hold the call (`rpc:<method>?aud=<did>#<service>`, a permission set that gives it, or `transition:generic`), or it fails `ScopeMissingError`, and its member must still hold `repo:*`. An app password's session calls anything but chat unless it may use chat. The host refuses methods that manage an account directly, itself as the service, and services not at a public address.

Signing an app in now keeps `transition:generic` and `transition:chat.bsky` for members holding `repo:*`, and other apps' permission sets (`include:social.grain.authFull?aud=…`) as far as the member's roles grant their parts. A set is read as its app publishes it: its `_lexicon` TXT record, read from `DNS_OVER_HTTPS` (Cloudflare's unless set), names the repo holding it.

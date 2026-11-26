---
default: minor
---

# An app signed in as a group reads its spaces

`com.atproto.space.getDelegationToken` answers an app signed in as a group, as a PDS answers one signed in as its account: a delegation token the group signs for one of its spaces, which buys a credential from `com.atproto.space.authorizeSpaceSync` to read the space, with `notify` if the app wants to hear of its changes. The session's scope must read the space (`space:<type>?action=read`, `ScopeMissing` otherwise), and the member who approved it must read it at that moment (`NotAuthorized`). So an app an admin approves once reads a group's `meta`, `members` and its own spaces, the members' records in them at their PDSes included, the Atmosphere Groups proposal's shared application session. Before, a group session could write in its spaces and read none of them.

---
default: minor
---

# A group's profile has the opensocial proposal's fields alone

`fyi.opensocial.profile` now holds what the proposal's `group.opensocial.profile` does: `displayName`, `description`, `avatar`, `banner`, `joinPolicy` and `url`, with `createdAt` beside them. `location`, `online`, `topics`, `languages` and `links` are gone, from the profile and from `getGroup`, with the `community.lexicon.location` lexicons. The console's profile settings no longer ask where a group meets, what it's about, its languages or its links, and its card no longer shows them. `opensocial profile set` and the `profile_set` MCP tool no longer take those fields, and nothing asks a favicon service for a link's icon, so the host, the console and the command no longer read `FAVICONS`. A profile written with the old fields keeps them in its record, but they aren't shown, and the console drops them when it next saves the profile.

---
default: minor
---

# The Apps page names apps as they name themselves, and says what was done in words

The apps acting as a group are listed by the name and icon their client metadata gives (`client_name`, `logo_uri`), with the domain of their client ID beside the name, what their scope lets them do ("Can post and make changes as the group", or "Can read the group's profile and members, and see requests to join" for an app that only reads), and who approved them. The app's icon shows on the approval page too, where an app asks to sign in as a group. The host reads the icon itself and serves it at `/img/app?id=<client ID>`: only an `https` `logo_uri` on the client ID's own origin, a PNG, JPEG, GIF or WebP by its bytes, 256 KiB at most, kept ten minutes. An app with none is drawn as its initial.

The card under them is now "Changes made as the group", made by apps and by members in the console, each said as it is seen: "@alice.test made the group members-only", "Grain (grain.social) created the group's Grain profile". A change to a space names it by what the group keeps there (its profile, its members, its pool), never by its URI, and a change to who may read it says so. Another app's record is named by its app and kind, with its lexicon's description under it where the host can resolve the lexicon. `org.atgrp.internal.listGroupTokens` gives what these are said from: each app's `client`, a space change's `space`, and a write's `description`.

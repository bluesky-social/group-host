---
default: major
---

# The host follows the Atmosphere Groups spec as released

The host now follows the Atmosphere Groups spec (tangled.org/atmospheregroups.org/spec) as it was released. Groups made before this change keep their old records, which the host no longer reads.

- A group's profile is now two records in its meta space: `org.atgrp.about` (display name, description, avatar and `website`, which was `url`) and `org.atgrp.config` (its join policy). The banner is gone. A group's rules and labeler moved to its members space. Its declaration is empty.
- Records and spaces were renamed: `acceptance` is now `confirmation`, and the `invites` inbox space is now `inbox`. In roles and space policies, `scopes` is now `permissions`, and in roles, `assignable` is now `canAssign`. A role's ID is lowercase letters and `-`, at least three of them.
- The permission sets are now `authManageBasic`, `authManageMembers`, `authModerate` and `authManageAdmin`. `authModerate` no longer admits, invites or assigns roles.
- Moderation methods are now under `org.atgrp.mod.*`: `submitReport`, `querySubjects`, `querySubjectHistory` and `updateSubject`. A subject is either open or resolved. Moderators tag subjects instead of escalating them, and can reopen a resolved one. Report reasons are `org.atgrp.mod.defs#reason*` tokens. Appeals go through the new `mod.submitAppeal`, and an account reads its own history with `mod.querySelfHistory`.
- An app signed in as the group no longer passes `group` to the group methods.
- `requestJoin` now returns the caller's existing request or membership, where before it refused them.
- `atgrp`: `role set` and `role grant` now take `--permissions` and `--can-assign`, `profile set` takes `--website`, `mod list` filters by tags and sorts, and `mod update` replaces `mod resolve`.

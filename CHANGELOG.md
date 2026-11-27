# Changelog

What changed in each version of the group host, its console and the
`opensocial` command for those who run and use them, from the change
files in `.changeset/` (see `knope.toml`).

## 0.4.1 (2026-11-27)

### Fixes

#### The relays are asked to read the host as it starts

The host asks each relay in `PDS_CRAWLERS` to read its firehose (`com.atproto.sync.requestCrawl`) as it starts, before it writes anything, and every 20 minutes after, whether or not it wrote anything. It asked only after writing an event, which a relay reading the host for the first time never received, so a new host's first group could miss the relays entirely. It logs `asked the relay to crawl` or `requestCrawl failed` for each relay each time.

## 0.4.0 (2026-11-26)

### Breaking Changes

#### The host follows the Atmosphere Groups spec as released

The host now follows the Atmosphere Groups spec (tangled.org/atmospheregroups.org/spec) as it was released. Groups made before this change keep their old records, which the host no longer reads.

- A group's profile is now two records in its meta space: `org.atgrp.about` (display name, description, avatar and `website`, which was `url`) and `org.atgrp.config` (its join policy). The banner is gone. A group's rules and labeler moved to its members space. Its declaration is empty.
- Records and spaces were renamed: `acceptance` is now `confirmation`, and the `invites` inbox space is now `inbox`. In roles and space policies, `scopes` is now `permissions`, and in roles, `assignable` is now `canAssign`. A role's ID is lowercase letters and `-`, at least three of them.
- The permission sets are now `authManageBasic`, `authManageMembers`, `authModerate` and `authManageAdmin`. `authModerate` no longer admits, invites or assigns roles.
- Moderation methods are now under `org.atgrp.mod.*`: `submitReport`, `querySubjects`, `querySubjectHistory` and `updateSubject`. A subject is either open or resolved. Moderators tag subjects instead of escalating them, and can reopen a resolved one. Report reasons are `org.atgrp.mod.defs#reason*` tokens. Appeals go through the new `mod.submitAppeal`, and an account reads its own history with `mod.querySelfHistory`.
- An app signed in as the group no longer passes `group` to the group methods.
- `requestJoin` now returns the caller's existing request or membership, where before it refused them.
- `atgrp`: `role set` and `role grant` now take `--permissions` and `--can-assign`, `profile set` takes `--website`, `mod list` filters by tags and sorts, and `mod update` replaces `mod resolve`.

### Features

#### Space URIs are at:space/

The host names a space `at:space/<authority>/<type>/<skey>`, and a record in one `at:space/<authority>/<type>/<skey>/<repo>/<collection>/<rkey>`, as the spaces alpha of November 24, 2026 does (@atproto/space 0.0.0-spaces-alpha-20261124235736). They were `at://<authority>/space/…`. Every method, the console, `atgrp` and the MCP tools answer and take the new form; a space or a record in one named the old way is refused, as anything that is not one is. The host renames the spaces its database keeps, and the moderation subjects and events that name them, when it starts.

#### An app signed in as a group reads its spaces

`com.atproto.space.getDelegationToken` answers an app signed in as a group, as a PDS answers one signed in as its account: a delegation token the group signs for one of its spaces, which buys a credential from `com.atproto.space.authorizeSpaceSync` to read the space, with `notify` if the app wants to hear of its changes. The session's scope must read the space (`space:<type>?action=read`, `ScopeMissing` otherwise), and the member who approved it must read it at that moment (`NotAuthorized`). So an app an admin approves once reads a group's `meta`, `members` and its own spaces, the members' records in them at their PDSes included, the Atmosphere Groups proposal's shared application session. Before, a group session could write in its spaces and read none of them.

#### An app signed in as a group calls its methods with rpc: scopes, and what its grant bought ends with it

Approval keeps `rpc:org.atgrp.listJoinRequests`, `denyJoinRequest`, `listModSubjects`, `getModSubjectHistory` and `resolveModSubject` (for this host or `aud=*`) when the approving member's roles hold the action each asks (`admit`, `mod.read`, `mod.resolve`). Those methods then answer the group session's DPoP token as well as a member's service auth, asking the member's roles again at each call (`Forbidden`), and the scope (`ScopeMissing`).

The delegation tokens a group session is given, and the space credentials and `notify` registrations they buy, are kept with its grant. When the grant ends (`revokeGroupToken`, `/oauth/revoke`, `deleteSession`, its member put out), or its member reads a space no more after a members-space write or a policy change, its credentials there are revoked, the writers' PDSes told, its registrations withdrawn, and its unspent delegations buy nothing.

#### /oauth/token names the approving member

A token response, to a code or a refresh, carries `member`, the DID of the member who approved the grant: this host's own field, beside `sub`, the group.

### Fixes

#### LastConfigurer says what to do, in words a person reads

`leaveGroup`, refused because nobody else could run the group, says "You're the group's only admin. Make someone else an admin first." A write to the members space that would leave nobody running it says "That would leave the group with no admin. Make someone else an admin first." `leaveGroup`'s lexicon describes `LastConfigurer` in the same words, so an app can show it to the person.

## 0.3.0 (2026-11-25)

### Features

#### The project is Atmosphere Groups

The lexicons are `org.atgrp.*`, and the host's own methods `org.atgrp.internal.*`; they were `fyi.opensocial.*`.

The command is `atgrp`, and reads `ATGRP_AS`, `ATGRP_PASSWORD`, `ATGRP_HOST`, `ATGRP_HOST_DID`, `ATGRP_SEED_INVITE` and `ATGRP_IMPORT_PASSWORD`. `just atgrp` runs it.

The host's DID document names its service `#atgrp`, of type `AtgrpGroupHost`, so an app proxies to `<did>#atgrp`.

The project is Atmosphere Groups, implementing the Atmosphere Groups proposal.

#### The host's own methods are internal, and groups no longer have websites

The methods that are this host's own, not the Atmosphere Groups proposal's, moved to `org.atgrp.internal.*`, with the views they answer in `org.atgrp.internal.defs`. A caller names them by their new NSIDs.

A group's website is gone: `putSite`, `deleteSite`, the `fyi.opensocial.site` record, the `fyi.opensocial.siteSpace` space type, the site kit and the page the host served around a site, `opensocial site` and the `site_publish` and `site_delete` MCP tools. A group's address now only redirects, to its own domain or to its page in the console.

A group's own domain stays, as `org.atgrp.internal.putGroupDomain` and `deleteGroupDomain`, `atgrp group domain` and the `group_domain` MCP tool, and the group's view names it as `domain`. `SITE_CHECK_AT` is now `DOMAIN_CHECK_AT`, and the host no longer reads `SITE_SHELL_DIR`, `SITE_HANDLE_RESOLVER` or `SITE_PLC_URL`.

`@opensocial/sdk` is gone.

### Fixes

#### A space read's signature and audience are refused as the reference refuses them

A space credential's request whose HTTP Message Signature fails, or whose `Atproto-Space-Audience` is missing or not a DID, is refused with 401 `BadSpaceSignature`, the spaces alpha's name, where it was `BadSignature`. A request signed for another repo than the one it reads is refused with 401 `BadSpaceAudience`, where it was 400, and only once its signature holds.

#### A space credential for another space is refused as InvalidCredential

A space read or an authority's method called with a sound space credential for another space is refused with 400 `InvalidCredential`, as the spaces alpha's reference refuses it, where it was 401 `BadJwtSub`. It is checked last, after the request's signature, revocation and audience, and a credential is looked up as revoked under the space it was issued for.

## 0.2.0 (2026-11-24)

### Features

#### A group's profile has the opensocial proposal's fields alone

`fyi.opensocial.profile` now holds what the proposal's `group.opensocial.profile` does: `displayName`, `description`, `avatar`, `banner`, `joinPolicy` and `url`, with `createdAt` beside them. `location`, `online`, `topics`, `languages` and `links` are gone, from the profile and from `getGroup`, with the `community.lexicon.location` lexicons. The console's profile settings no longer ask where a group meets, what it's about, its languages or its links, and its card no longer shows them. `opensocial profile set` and the `profile_set` MCP tool no longer take those fields, and nothing asks a favicon service for a link's icon, so the host, the console and the command no longer read `FAVICONS`. A profile written with the old fields keeps them in its record, but they aren't shown, and the console drops them when it next saves the profile.

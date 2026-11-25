# Changelog

What changed in each version of the group host, its console and the
`opensocial` command for those who run and use them, from the change
files in `.changeset/` (see `knope.toml`).

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

# HTTP and XRPC API

This is what the host answers over HTTP, at `PUBLIC_URL` ([config](config.md)).
Each `org.atgrp.*` method has its lexicon under
`lexicons/org/atgrp`, which says what it takes and answers, and the
docs site shows it under the method. The `org.atgrp.internal.*`
methods, under `lexicons/org/atgrp/internal`, are this host's own,
for its console, its command and its operators. They aren't part of the
Atmosphere Groups spec, and may change. The design pages say why each part
works the way it does.

An XRPC error is JSON, the same as atproto's servers send: an HTTP status
and `{"error", "message"}`. Besides the names a method's lexicon lists,
any method may answer these:

| Error                                                          | When                                                                                                                                                                                                                                                   |
| -------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `InvalidRequest`                                               | a parameter is missing or bad. The host checks a `org.atgrp.*` method's parameters and JSON input against its lexicon before anything else, and says where and what is wrong (`input.roles[2].role must not be longer than 64 bytes`)             |
| `PayloadTooLarge`                                              | a JSON input is over 2 MiB                                                                                                                                                                                                                             |
| `AuthMissing`                                                  | the method needs a caller and there isn't one                                                                                                                                                                                                          |
| `BadJwt`, `BadJwtAudience`, `BadJwtIss`, `BadJwtLexiconMethod` | service auth doesn't hold up: a bad token, a token for another service, a signer whose key can't be found, or a token for another method                                                                                                               |
| `Forbidden`                                                    | the caller's roles lack the action (`missing <action>`)                                                                                                                                                                                                |
| `GroupNotFound`                                                | the DID isn't a group here                                                                                                                                                                                                                             |
| `RepoDeactivated`                                              | the group is switched off                                                                                                                                                                                                                              |
| `RateLimitExceeded`                                            | a 429: the caller has asked more than a [limit](../design/limits.md) allows in its window. `Retry-After` says in how many seconds to ask again, and `RateLimit-Limit`, `RateLimit-Remaining`, `RateLimit-Reset` and `RateLimit-Policy` say which limit |
| `InternalServerError`                                          | the host's database, or a server it asked, failed                                                                                                                                                                                                      |

The atproto methods answer with atproto's own names (`RepoNotFound`,
`BlobNotFound`, `InvalidSwap`, `CredentialRevoked`, …). When an app
writes a record as a group in one of our own `org.atgrp.*`
collections, the host checks it against its lexicon and answers
`validationStatus: "valid"`. It keeps a record of any other collection
without reading it and answers `unknown`. A GET or POST under `/xrpc/`
that no route answers gets 501 `MethodNotImplemented`, unless it carries
`atproto-proxy` and a group's token: then the host sends it on
([signing in](../design/signing-in.md#calling-other-services-as-the-group)).

## Who asks

The host's own methods know their caller in one of two ways:

- Service auth. That's `Authorization: Bearer <jwt>`, from the caller's
  own PDS (`com.atproto.server.getServiceAuth`), with the host's
  `SERVICE_DID` as `aud` (or `SERVICE_DID#<service>`, the way a PDS
  proxying to the host's `#atgrp` service names it) and the
  method's NSID as `lxm`. An app, or the `atgrp` command, calls as
  a person this way.
- The console's cookie, `__Host-group_host_session` (`group_host_session`
  where the console isn't on https). The host signs it once someone
  signs in to the console ([signing in](../design/signing-in.md#signing-people-in-to-the-console)).
  The host uses it only when no `Authorization` is sent, and only from
  the console's own pages. A request with another `Origin` counts as
  coming from nobody.

What a caller may then do in a group is what their roles there allow
([membership](../design/membership.md)), and a method's lexicon names the
permission it needs. An app signed in as a group, with the host's OAuth
([signing in](../design/signing-in.md)), writes to the group's repo and
spaces through the atproto methods below.

## Groups

| Method                                  | What it does                                                                                                                                                                                                                              |
| --------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `POST org.atgrp.internal.createGroup`       | starts a group, with the caller as its founder and first admin. `metaPublicRead: false` keeps its `meta` space from anyone but its members                                                                                                |
| `GET org.atgrp.internal.getGroup`           | shows a group as the caller may see it. Someone signed in who may read `meta` gets its about record (`displayName`, `description`, `avatar`, `website`) and its config's `joinPolicy`. One who may read `members` also gets its rules, its labels, its member count, roles and spaces. A member is told, as `grantable`, which actions' permissions and permission sets they hold enough of to give a role, and as `labelSpaces` the spaces where their roles write labels. Someone not signed in gets only its DID and handle |
| `GET org.atgrp.internal.checkAddress`       | says whether an address (one label under `HANDLE_DOMAIN`) is free for a new group. Anyone may ask                                                                                                                                         |
| `GET org.atgrp.internal.checkGroupHandle`   | says whether a domain names the group yet, so it can be its handle                                                                                                                                                                        |
| `POST org.atgrp.internal.updateGroupHandle` | gives the group a handle on its own domain, or gives its address back                                                                                                                                                                     |
| `POST org.atgrp.internal.putGroupDomain`    | serves the group at its handle's own domain once that domain answers as this host. Its address and `www.` under the domain redirect there (`NoOwnDomain`, `NotReaching`) ([its own domain](../design/groups.md#its-own-domain)) |
| `POST org.atgrp.internal.deleteGroupDomain` | stops serving the group at its own domain                                                                                                                                                                                                 |
| `POST org.atgrp.internal.deactivateGroup`   | switches a group off until `activateGroup`                                                                                                                                                                                                |
| `POST org.atgrp.internal.activateGroup`     | switches a deactivated group back on                                                                                                                                                                                                      |
| `POST org.atgrp.internal.deleteGroup`       | deletes a group for good ([groups](../design/groups.md))                                                                                                                                                                                  |

A group's address, and its own domain once it's served there, answer
these too. A request to the host's own name never does:

| Route | What it does |
| --- | --- |
| `GET`, `HEAD` `https://<address>/` | redirects to the group's page in the console. Any other path, besides `/.well-known/` and `/xrpc/`, which the host keeps for itself there, is a plain 404 |
| `https://<address>/<path>`, `https://www.<domain>/<path>` | while the group is served at its own domain, redirect (301) to the same path and query there |
| `GET /tls/ask?domain=<domain>` | answers 200 if the proxy in front may get a certificate for `domain`, and 404 otherwise. A domain qualifies if it's a group's own domain, or `www.` under it, while the group is on |

## Membership

| Method | What it does |
| --- | --- |
| `GET org.atgrp.internal.listMembers` | lists the group's members and their roles, a page at a time in DID order (`limit`, `cursor`), filtered to those whose handle or DID contains `q`. It's for a caller who may read the members space |
| `POST org.atgrp.leaveGroup` | leaves a group (`GroupNotFound`, `NotAMember`), unless the caller is its only admin (`LastConfigurer`) |
| `POST org.atgrp.requestJoin` | asks to join, or redeems an `invite` in the caller's inbox. It answers `status` `admitted` with the `membership`, or `pending` with the `request`. A caller who already asked gets their waiting request back, and a member gets their membership (`GroupNotFound`, `InvalidInvite`, `JoinClosed`) |
| `GET org.atgrp.internal.getJoinRequest` | shows the caller's own request to join while it waits |
| `POST org.atgrp.cancelJoinRequest` | withdraws that request (`GroupNotFound`, `JoinRequestNotFound`) |
| `GET org.atgrp.listJoinRequests` | lists the waiting requests, for those who admit people (`InvalidCursor`) |
| `POST org.atgrp.denyJoinRequest` | turns a request down, for the same (`JoinRequestNotFound`) |
| `GET org.atgrp.internal.listInvites` | lists the group's invites that are still waiting, each with the invitee's handle |
| `GET org.atgrp.internal.getActorInvites` | lists the caller's own invites from groups here |
| `POST org.atgrp.internal.declineInvite` | turns an invite down |

A group's roles, what each may do, and who holds them are records in its
members space: `org.atgrp.role`, each with its `permissions` and the
roles it may assign (`canAssign`, `*` for every role), and `org.atgrp.membership`.
A role's ID is lowercase letters and `-`, three of them at least.
They're written as the group with the `com.atproto.space.*` writes
[below](#signing-apps-in-as-a-group). Writing a membership for someone
waiting to join lets them in. Writing one for a member changes their
roles, and deleting one puts the member out
([membership](../design/membership.md#writing-roles-and-memberships)).

| Refusal | Why |
| --- | --- |
| `LastConfigurer` | nobody would be left holding a role that configures the group |
| `RoleInUse` | a deleted role is still held |
| `NotAssignable` | the writer may not assign a role being given or taken, or one whose grant is changing, or may not give every role (`*`) |
| `ScopeNotHeld` | the writer's roles don't hold a permission being given |
| `MemberRole` | a membership leaves out `member` |
| `JoinRequestNotFound` | a membership is made for someone who isn't waiting to join |

A group invites someone by writing an `org.atgrp.invite` record in
their inbox as the group. It takes the invite back by deleting it, with
the same writes ([membership](../design/membership.md#invites)).

## People

| Method                              | What it does                                                                                                                                                                                                                   |
| ----------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `GET org.atgrp.internal.getViewer`      | says who's asking, by the console's cookie or service auth. It also lists the other accounts the browser is signed in to (`accounts`), and where someone without an account makes one (`signUpAt`, `SIGNUP_PDS`)               |
| `GET org.atgrp.internal.getActor`       | gives a person's DID and handle to someone signed in, about themselves or anyone the host shows (a member of a group here, or someone asking to join one). Asked about themselves, it also says whether they may start a group |
| `GET org.atgrp.internal.getActorGroups` | lists the groups here a person is in, as far as the caller shares them                                                                                                                                                         |
| `GET org.atgrp.internal.searchActors`   | finds accounts whose handle matches what's typed, from the source `ACTOR_SEARCH` names. Anyone may ask                                                                                                                         |
| `GET /img/<did>/<cid>`              | serves a group's or a person's image to anyone who may see it. That's anyone for an image in a public repo, and otherwise only someone signed in to the console. One the group has taken down in a space, by its writer or a record holding it, it serves to nobody. A moderator reads it through `getModSubjectBlob` |

## Spaces

A group's spaces, and the records in them ([spaces](../design/spaces.md)).

| Method | What it does |
| --- | --- |
| `GET org.atgrp.internal.listSpaceRecords` | lists the records in a space from the host's index of it, for someone signed in who may read it, but none the group has taken down with a `!takedown` label, for a moderator as for anyone |
| `GET org.atgrp.internal.getSpaceBlob` | serves a file in a space, read from the repo of whoever wrote it, unless the group has taken down its writer or a record holding it |

The host is the spaces' authority and the PDS of the group's own repo in
each, so it answers the space alpha's methods:

| Method                                                                                                                                                                                                                                                    | What it does                                                                                                                                                                                                                                                                                                                                                                                                     |
| --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `POST com.atproto.space.authorizeSpaceSync`                                                                                                                                                                                                               | gives a credential to read a space, bound to `credentialKey`, with the space's revision (`spaceRev`). It gives one to a user it lets in (`delegationToken`), or to anyone for a space open to all. With `notify`, it registers an endpoint, once the endpoint confirms, to hear of changes until the credential lapses (`SpaceDeleted`, `RepoDeactivated`, `InvalidDelegationToken`, `RegistrationNotConfirmed`) |
| `POST com.atproto.space.notifyRepoUpdate`                                                                                                                                                                                                                 | hears about a member's write in a space, by service auth from the writer (`FutureRev`)                                                                                                                                                                                                                                                                                                                           |
| `GET com.atproto.space.listWriters`                                                                                                                                                                                                                       | lists the space's writer set in the order of its revisions, each a `repoWrite` or a `repoRemoved`. A page holds up to 1000 (100 by default), and `cursor` is the last one's `spaceRev`                                                                                                                                                                                                                           |
| `POST com.atproto.space.notifyCredentialsRevoked`                                                                                                                                                                                                         | hears from a space's authority that credentials no longer hold                                                                                                                                                                                                                                                                                                                                                   |
| `GET com.atproto.space.getRecord`, `GET com.atproto.space.listRecords`, `GET com.atproto.space.getRepo`, `GET com.atproto.space.getLatestCommit`, `GET com.atproto.space.listRepoOps`, `GET com.atproto.space.getBlob`, `GET com.atproto.space.listBlobs` | serve the group's own repo in a space, for a credential that may read it. `getRecord`, `getRepo` (a STAR archive, `application/vnd.dasl.star`), `getLatestCommit` and `listRepoOps` (its `commit` on every page) need a MAC asked for in `Accept-Signature` (`InvalidSpaceMac`). A malformed cursor fails with `InvalidCursor`                                                                                   |

The host sends `com.atproto.space.notifySpaceUpdate` to the endpoint of
a service registered for a space. It sends one for each change to the
writer set, for a lapsed registration, and when the space is deleted
([spaces](../design/spaces.md#hearing-of-writes)).

## Signing apps in as a group

The host is each group's OAuth server ([signing in](../design/signing-in.md)):

| Route                                                                                      | What it does                                                                       |
| ------------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------- |
| `GET /.well-known/oauth-authorization-server`, `GET /.well-known/oauth-protected-resource` | its metadata                                                                       |
| `POST /oauth/par`, `GET /oauth/authorize`, `POST /oauth/token`, `POST /oauth/revoke`       | atproto OAuth's endpoints. `authorize` sends the browser to the console to approve |

| Method                                     | What it does                                                                                                                                                                                                                                                                                                                                                                                                                    |
| ------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `POST org.atgrp.internal.provisionGroup`       | lets an app that `PROVISIONING_CLIENTS` names start a group for a person, and signs the app in to it. It takes `metaPublicRead` as `createGroup` does                                                                                                                                                                                                                                                                           |
| `POST org.atgrp.internal.getGroupAuth`         | gives a member with a DPoP proof a 15-minute token to act as the group. In one of the group's spaces, it's scoped to what their roles may write there as the group, and to the `manage=` operations they may use on it. For a space not made yet, that's `manage=create`, if their roles hold `space.create`. In someone's inbox, it lets a member who may invite write invites there. It answers `Forbidden` if that's nothing |
| `GET org.atgrp.internal.getAuthorization`      | shows an app's request to sign in as a group, as the caller would approve it                                                                                                                                                                                                                                                                                                                                                    |
| `POST org.atgrp.internal.approveAuthorization` | approves the request with what the caller's roles allow                                                                                                                                                                                                                                                                                                                                                                         |
| `POST org.atgrp.internal.declineAuthorization` | declines the request                                                                                                                                                                                                                                                                                                                                                                                                            |
| `GET org.atgrp.internal.listGroupTokens`       | lists the apps signed in as the group and what they did lately, each with the handle of whoever let it in. A write also comes in its parts (`write`: operation, collection, key, space) and with whom it's about (`subject`)                                                                                                                                                                                                    |
| `POST org.atgrp.internal.revokeGroupToken`     | signs one app out                                                                                                                                                                                                                                                                                                                                                                                                               |
| `POST org.atgrp.internal.createAppPassword`    | makes an app password for the group, for an app that signs in with one                                                                                                                                                                                                                                                                                                                                                          |
| `GET org.atgrp.internal.listAppPasswords`      | lists the group's app passwords, each with its owner's handle. It never shows the passwords                                                                                                                                                                                                                                                                                                                                     |
| `POST org.atgrp.internal.revokeAppPassword`    | revokes one, and signs out its apps                                                                                                                                                                                                                                                                                                                                                                                             |

An app signed in this way, or with an app password, uses these as the
group:

| Method                                                                                                                                                                                 | What it does                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `GET com.atproto.server.getSession`                                                                                                                                                    | describes the group a token acts as: its DID, its handle, and whether it's switched on                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| `POST com.atproto.server.createSession`, `POST com.atproto.server.refreshSession`, `POST com.atproto.server.deleteSession`                                                             | handle an app password's sessions                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| `POST com.atproto.repo.createRecord`, `POST com.atproto.repo.putRecord`, `POST com.atproto.repo.deleteRecord`, `POST com.atproto.repo.applyWrites`, `POST com.atproto.repo.uploadBlob` | write to the group's public repo                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| `POST com.atproto.space.createRecord`, `POST com.atproto.space.putRecord`, `POST com.atproto.space.deleteRecord`, `POST com.atproto.space.applyWrites`                                 | write to the group's repo in a space. That's one of its own spaces, or someone's inbox, where it writes and deletes its invites to them (`InviteNotFound`, and `InvalidRequest` for an invite to a member). A label (`com.atproto.label.record`) is written and deleted in any of the group's spaces, beside its subject. A label on a member's record in their public repo goes in the members space ([moderation](../design/moderation.md#labels))                                                                                                                                                                                     |
| `POST com.atproto.space.createSpace`                                                                                                                                                   | makes a space of `spaceType` (and `skey`, or a new one) under `policy`, an `org.atgrp.policy`. It publishes the policy and lists the space in the index unless the policy sets `unlisted`, and answers the `space`. It needs `manage=create` in the scope (`ScopeMissing`) and a role holding `space.create` (`NotAuthorized`). It refuses another policy type (`UnsupportedPolicy`), the meta and members types, a grant of a role the group lacks, two grants of one role, and a grant whose `permissions` include one that's no space permission (`InvalidRequest`) ([spaces](../design/spaces.md#making-changing-and-ending-a-space)) |
| `POST com.atproto.space.updateSpace`                                                                                                                                                   | puts the group's `space` under a new `policy`, published, and listed or unlisted as it says. It needs `manage=update` in the scope (`ScopeMissing`), and a role holding `space.configure` or a `manage=update` permission in the space's own policy (`NotAuthorized`). It refuses changing the grant of a role the caller may not assign, adding a permission their roles don't hold, and granting `read` or `write`, or turning on `publicRead`, where they don't have it (`NotAuthorized`). On the members space, it refuses a policy that takes `read` or `write` from `member` or turns on `publicRead` (`MembersSpaceFixed`). It refuses a policy as `createSpace` does, and a space the group doesn't have (`SpaceNotFound`) |
| `POST com.atproto.space.deleteSpace`                                                                                                                                                   | ends the group's `space`: out of the index, its syncers told, and the group's repo in it gone. It needs `manage=delete` in the scope (`ScopeMissing`), and a role holding `space.delete` or a `manage=delete` permission in the space's policy (`NotAuthorized`). The meta and members spaces are never deleted (`NotAuthorized`), and a space the group doesn't have fails `SpaceNotFound`                                                                                                                                                                                                                    |
| `GET com.atproto.space.getSpace`                                                                                                                                                       | answers the group's `space` and its `policy`, for a scope that reads it (`ScopeMissing`) and a member who reads it or may change it (`NotAuthorized`, `SpaceNotFound`)                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| `GET com.atproto.space.getDelegationToken` | answers a delegation `token` the group signs for its `space`, which buys a credential from `authorizeSpaceSync` to read the space as any reader does: the group's repo there, here, and its members' repos at their PDSes. It's for a scope that reads the space (`ScopeMissing`) and a member who reads it themselves at that moment (`NotAuthorized`), in one of the group's own spaces (`SpaceNotFound`) |
| `GET com.atproto.server.getServiceAuth` | answers a `token`, service auth the group signs with its key, for `aud` (a DID, or `did#service`), `lxm` if given, lapsing at `exp` (a minute from now unless asked; within an hour for one method, a minute for any: `BadExpiration`). A token's scope must hold calling `lxm` (or any method) at `aud` (`ScopeMissingError`), and its member still hold `repo:*` (`Forbidden`). An app password's session gets one for anything but chat and `createAccount`, unless it may use chat. Never for a method managing an account directly (`InvalidRequest`) |
| any other method, with `atproto-proxy: <did>#<service>` | sent on to that service, signed as the group (`aud` the service's DID, `lxm` the method, a minute), and its answer streamed back, as an account's PDS does. Refused as `getServiceAuth` refuses, and with `InvalidToken` for a method managing an account directly or, for an app password that may not, chat; `InvalidRequest` for a header that doesn't parse, a service its document doesn't name, or this host itself; `UpstreamFailure` for a service that isn't at a public address or doesn't answer |

A group's about record and config, in `meta`, and its rules and roles,
in `members`, change only this way. An app writes the records as the
group, and the host keeps each one only as the rules allow
([groups](../design/groups.md#changing-its-about-record-rules-and-pictures)).

## MCP

An MCP client (an AI app) connects to a group over MCP ([MCP](../design/mcp.md)).
Its tools are in [MCP tools](mcp.md).

| Route                                                                                              | What it does                                                                               |
| -------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------ |
| `POST /mcp`                                                                                        | the MCP server, which answers each JSON-RPC message as the member who connected the client |
| `GET /.well-known/oauth-protected-resource/mcp`, `GET /.well-known/oauth-authorization-server/mcp` | its metadata                                                                               |
| `POST /mcp/register`, `GET /mcp/authorize`, `POST /mcp/token`                                      | its own OAuth server                                                                       |

| Method                                   | What it does                                          |
| ---------------------------------------- | ----------------------------------------------------- |
| `GET org.atgrp.internal.listConnectedApps`   | lists the MCP clients the caller connected to a group |
| `POST org.atgrp.internal.revokeConnectedApp` | disconnects one                                       |

## Moderation

| Method | What it does |
| --- | --- |
| `POST org.atgrp.mod.submitReport` | reports a member, a record in one of the group's spaces here, or a member's record in their public repo: `subject` is an account's DID or a record's URI, with an optional `cid`. The reporter must be able to read its space (the members space for a member or their public record), and the group's labeler must take reports of its kind and reason. `reasonType` is one of the `org.atgrp.mod.defs#reason*` tokens (`reasonSpam`, `reasonViolation`, `reasonMisleading`, `reasonNsfw`, `reasonRude`, `reasonOther`), and a `rule` it cites is a rule in the members space. It answers the report's `eventId` and `createdAt` (`GroupNotFound`, `InvalidSubject`) |
| `POST org.atgrp.mod.submitAppeal` | appeals a label or an ejection on the caller's own account or record, with an optional `note`, and reopens the subject. It takes an appeal from someone who has left the group too. It answers the appeal's `eventId` and `createdAt` (`GroupNotFound`, `InvalidSubject` for a subject that isn't the caller's, `NotAppealable` for one with no live label or ejection) |
| `GET org.atgrp.mod.querySelfHistory` | lists what the group did to the caller's account and records, or to one `subject` of theirs, newest first unless `sort` is `oldest`. It leaves out reports, tags, notes and who acted, and works for former members (`GroupNotFound`, `InvalidSubject`, `InvalidCursor`) |
| `GET org.atgrp.mod.querySubjects` | lists the moderation queue: every status unless `status` is `open` or `resolved`, and only subjects with any of `tags` when given. `sort` is `oldest` (the default) or `newest` by when the subject entered the queue, or `updated`, the latest changed first (`InvalidCursor`) |
| `GET org.atgrp.mod.querySubjectHistory` | lists a subject's events, newest first unless `sort` is `oldest`, a page at a time. With a `cid`, it lists those about that version. For a record in one of the group's spaces that the caller may read, it also answers the `record` as the host's index holds it, with `takenDown` true if the group has taken that version down (`InvalidSubject`, `InvalidCursor`) |
| `GET org.atgrp.internal.getModSubjectBlob` | serves a file the record `subject` holds, by its `cid`, to someone with `mod.read` whom the record's space lets read it, even if the group has taken the record down. A file the record doesn't hold, or a record the caller can't read there, fails with `NotFound` |
| `POST org.atgrp.mod.updateSubject` | changes a subject in the queue: its `status` (`open` or `resolved`), a `note`, and its tags (`addTags`, `removeTags`, lowercase letters and `-`, 25 at most each). What it leaves out stays as it is. It answers the subject (`InvalidSubject`, `SubjectNotFound`) |
| `POST org.atgrp.internal.putLabelDefinition` | defines or changes one of the group's own labels (lowercase letters and hyphens) in its labeler, for someone with `group.configure`. Its name and description are in `lang` (`en` unless given), and replace only those in that language. It answers the definition, and `getGroup` answers them as `labels` |
| `POST org.atgrp.internal.deleteLabelDefinition` | stops defining one. Labels already applied stay |

A report's `reason`, and an appeal's or update's `note`, is 2,000
characters at most (20,000 bytes). Reports and appeals share a limit of
10 in 5 minutes and 100 a day for each caller
([limits](../design/limits.md)).

`mod.querySubjects`, `mod.querySubjectHistory` and `mod.updateSubject`,
like `listJoinRequests` and `denyJoinRequest`, answer a member by service
auth or the console's cookie, who names the group as `group`
(`InvalidRequest` without it). They also answer an app signed in as the
group (a DPoP token) whose scope names the method (`rpc:`, or a set
listing it), as its member, while their roles hold the method's action
([signing in](../design/signing-in.md#narrowing-at-approval)). That app
leaves `group` out, since its session names it, and one naming another
group gets `Forbidden`.

## Operating the host

These are for its operators, the accounts `HOST_ADMINS` names
([config](config.md)), by service auth or the console's cookie. The host
refuses anyone else with `Forbidden`.

| Method                        | What it does                                                                                                                                                                                                                                                                                                                                                |
| ----------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `GET org.atgrp.internal.getLogs`  | returns the lines the host logged, oldest first, each as it wrote it. It returns one request's lines by its `trace`, or the last lines of a `message`, at a `level` and above, or that took `minMs` or longer. It returns 100 unless `limit` asks for up to 500, and 500 for a trace. The read itself is logged as `logs read`, with who asked and for what |
| `GET org.atgrp.internal.getStats` | shows how the host was used over the last `days` (7 by default, 400 at most), day by day in UTC, and what it holds. It counts requests by route and by status class, slow ones, MCP tools called by name, and groups and spaces made. Nothing in it names who asked                                                                                         |

The host logs every request as a `request` line, with its method, route,
path (never its query), status and `duration_ms`. When it refuses or
fails a request, the line also has the `error` and `error_message` it
answered. Each response carries `x-trace-id`, the trace that every line
logged while answering it carries. The trace is the host's own, or the
one a W3C `traceparent` header names. Since the caller names that one, a
trace read may hold the lines of another request that named it too. A
method other than HTTP's own seven is counted as `OTHER`.

## Recovery

| Method                                  | What it does                                                                                     |
| --------------------------------------- | ------------------------------------------------------------------------------------------------ |
| `GET org.atgrp.internal.listRecoveryKeys`   | lists the rotation keys on the group's `did:plc` that matter ([recovery](../design/recovery.md)) |
| `POST org.atgrp.internal.addRecoveryKey`    | adds a steward's key, ahead of the host's                                                        |
| `POST org.atgrp.internal.removeRecoveryKey` | takes one off                                                                                    |

## Moving

Moving groups between hosts, and accounts here to become groups ([moving](../design/moving.md)).

| Method                                   | What it does                                                       |
| ---------------------------------------- | ------------------------------------------------------------------ |
| `POST org.atgrp.internal.migratePrepare`     | on the host a group leaves, starts the move                        |
| `GET org.atgrp.internal.migrateExport`       | on that host, serves what the destination pulls of the group       |
| `POST org.atgrp.internal.migrateReceive`     | on the destination, takes the group in, switched off               |
| `POST org.atgrp.internal.migrateComplete`    | on the host it leaves, points its DID at the destination           |
| `POST org.atgrp.internal.migrateActivate`    | on the destination, switches it on                                 |
| `GET org.atgrp.internal.checkImport`         | shows what moving an account here would make of it                 |
| `POST org.atgrp.internal.importAccount`      | starts moving an account here                                      |
| `POST org.atgrp.internal.importAccountSpace` | brings over one of its spaces while the move waits                 |
| `POST org.atgrp.internal.completeImport`     | finishes the move                                                  |
| `GET org.atgrp.internal.getAccountMove`      | shows the account the caller signed in to from the console to move |
| `POST org.atgrp.internal.startAccountMove`   | starts that move                                                   |
| `POST org.atgrp.internal.finishAccountMove`  | finishes it with the code the account's server emailed             |
| `POST org.atgrp.internal.cancelAccountMove`  | stops it                                                           |

## The console

| Route                                                                 | What it does                                                                                                                                                                                                     |
| --------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `GET /console/login`, `GET /console/callback`, `POST /console/logout` | sign in to the console as yourself (`?handle=`), sign up at `SIGNUP_PDS` (`?create`), and sign out. Signing in again keeps the earlier account beside the new one, and signing out switches to the one used last |
| `POST /console/switch`                                                | switches to another account the browser is signed in to (`did`), and goes back to the page `back`                                                                                                                |
| `GET /console/move/login`                                             | signs in to an account to move here                                                                                                                                                                              |
| `GET /console/client-metadata.json`                                   | the host's metadata as the console's OAuth client                                                                                                                                                                |

## The PDS

The host is the PDS of every group it hosts, and answers for their public
repos as any PDS does:

| Method                                                                                                                                                                     | What it does                                                                                                                                                               |
| -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `GET com.atproto.server.describeServer`                                                                                                                                    | describes the host as a PDS: its DID, the domain it names its groups under, `BLOB_UPLOAD_LIMIT`, and that no one signs up                                                  |
| `GET com.atproto.repo.describeRepo`, `GET com.atproto.repo.getRecord`, `GET com.atproto.repo.listRecords`                                                                  | serve a group's public repo                                                                                                                                                |
| `GET com.atproto.sync.getRepo`, `GET com.atproto.sync.getRecord`, `GET com.atproto.sync.getLatestCommit`, `GET com.atproto.sync.listRepos`, `GET com.atproto.sync.getBlob`, `GET com.atproto.sync.listBlobs` | sync the repos. `getRecord` answers a CAR rooted at the repo's signed commit, with the tree's nodes proving the record, and the record when it is there; a record not there is proved absent the same way. A blob only a space holds isn't served or listed. `listRepos` and `listBlobs` answer pages of up to 1000 (500 by default), as atproto's lexicons have them |
| `GET com.atproto.sync.subscribeRepos`                                                                                                                                      | the firehose, over a WebSocket                                                                                                                                             |
| `GET com.atproto.identity.resolveHandle`                                                                                                                                   | resolves a handle here to its DID. For a caller who's signed in, it resolves any handle through `HANDLE_RESOLVERS`                                                         |
| `GET /.well-known/did.json`                                                                                                                                                | the host's own DID document when `SERVICE_DID` is a `did:web`, with its `#atproto_pds` and `#atgrp` services at `PUBLIC_URL`                                          |
| `GET /.well-known/atproto-did`                                                                                                                                             | the DID of the group whose address the request's host names                                                                                                                |
| `GET /xrpc/_health`                                                                                                                                                        | says the host is up, with its `version` and the `commit` its image was built from (`null` in a build of a checkout)                                                        |

A switched-off group's public repo answers `RepoDeactivated`, and
`listRepos` shows it as `active: false`.

## From a browser

The OAuth routes, the `/mcp` routes, and the methods an app calls with a
token (the writes, the sessions, `provisionGroup`, `getGroupAuth`) answer
any origin. So do the methods a space is read by. Those are its
authority's `authorizeSpaceSync` and `listWriters`, and the reads of a
repo in it (`getRepo`, `listRepoOps` and the rest), which an app's page
makes. The rest are for the console,
at its own origin.

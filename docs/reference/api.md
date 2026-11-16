# HTTP and XRPC API

What the host answers over HTTP, at `PUBLIC_URL` ([config](config.md)).
Each `fyi.opensocial.*` method has its lexicon under
`lexicons/fyi/opensocial`, which says what it takes and answers; the site
shows it under the method. The design pages say why each part works as it
does.

An XRPC error is JSON, as atproto's servers answer one: an HTTP status and
`{"error", "message"}`. Besides the names a method's lexicon lists, any
method may answer these:

| Error | When |
| --- | --- |
| `InvalidRequest` | a missing or bad parameter: a `fyi.opensocial.*` method's parameters and JSON input are checked against its lexicon before anything else, and refused with where and what, `input.roles[2].role must not be longer than 64 bytes` |
| `PayloadTooLarge` | a JSON input of more than 2 MiB, or for `putSite`, more than `BLOB_UPLOAD_LIMIT` |
| `AuthMissing` | no caller, where one is needed |
| `BadJwt`, `BadJwtAudience`, `BadJwtIss`, `BadJwtLexiconMethod` | service auth that does not hold: a bad token, one for another service, a signer whose key cannot be found, or one for another method |
| `Forbidden` | a caller whose roles lack the action, `missing <action>` |
| `GroupNotFound` | a DID that is no group here |
| `RepoDeactivated` | a group switched off |
| `InternalServerError` | the host's database, or a server it asked, failed |

The atproto methods answer atproto's own names (`RepoNotFound`,
`BlobNotFound`, `InvalidSwap`, `CredentialRevoked`, …). A record an app
writes as a group, in one of our own `fyi.opensocial.*` collections, is
checked against its lexicon and answered `validationStatus: "valid"`; a
record of any other collection is kept unread, `unknown`. A GET or POST under
`/xrpc/` that no route answers is 501 `MethodNotImplemented`.

## Who asks

A method of the host's own knows its caller in one of two ways:

- **Service auth**: `Authorization: Bearer <jwt>`, from the caller's own
  PDS (`com.atproto.server.getServiceAuth`), with the host's `SERVICE_DID`
  as `aud` (or `SERVICE_DID#<service>`, as a PDS proxying to the
  host's `#opensocial` service names it) and the method's NSID as `lxm`. This is how an app, or the
  `opensocial` command, calls as a person.
- **The console's cookie**: `__Host-group_host_session` (`group_host_session`
  where the console is not on https), which the host signs once someone
  signs in to the console ([signing in](../design/signing-in.md#signing-people-in-to-the-console)),
  used only when no `Authorization` is sent, and only from the console's
  own pages: a request whose `Origin` is another is as nobody's.

What a caller may then do in a group is what their roles there allow
([membership](../design/membership.md)); a method's lexicon names the
permission it needs. An app signed in *as* a group, with the host's OAuth
([signing in](../design/signing-in.md)), writes to the group's repo and
spaces through the atproto methods below.

## Groups

| Method | What it does |
| --- | --- |
| `POST fyi.opensocial.createGroup` | starts a group, the caller its founder and first admin |
| `GET fyi.opensocial.getGroup` | a group as the caller may see it: its profile and rules, and to a member who may read them how many members it has, its roles and spaces; to someone not signed in, its DID and handle alone |
| `GET fyi.opensocial.checkAddress` | whether an address, one label under `HANDLE_DOMAIN`, is free for a new group; anyone may ask |
| `GET fyi.opensocial.checkGroupHandle` | whether a domain names the group yet, to be its handle |
| `POST fyi.opensocial.updateGroupHandle` | gives the group a handle on its own domain, or its address back |
| `POST fyi.opensocial.deactivateGroup` | switches a group off, until `activateGroup` |
| `POST fyi.opensocial.activateGroup` | switches a deactivated group back on |
| `POST fyi.opensocial.deleteGroup` | deletes a group, for good ([groups](../design/groups.md)) |

## Membership

| Method | What it does |
| --- | --- |
| `GET fyi.opensocial.listMembers` | the group's members and their roles, a page at a time in the order of their DIDs (`limit`, `cursor`), narrowed to those whose handle or DID holds `q`; for a caller who may read the members space |
| `POST fyi.opensocial.leaveGroup` | leaves a group, unless no one else could configure it (`LastConfigurer`) |
| `POST fyi.opensocial.requestJoin` | asks to join, or redeems an invite |
| `GET fyi.opensocial.getJoinRequest` | the caller's own request to join, while it waits |
| `POST fyi.opensocial.cancelJoinRequest` | withdraws it |
| `GET fyi.opensocial.listJoinRequests` | the requests waiting, for those who admit |
| `POST fyi.opensocial.denyJoinRequest` | turns a request down |
| `GET fyi.opensocial.listInvites` | the group's invites still waiting, each with whom it invites by their handle |
| `GET fyi.opensocial.getActorInvites` | the caller's own invites from groups here |
| `POST fyi.opensocial.declineInvite` | turns an invite down |
| `GET com.atproto.managedspace.checkAccess` | whether an account (`repo`) may read or write an inbox the host keeps, asked by its authority; `{read, write}` |

A group's roles, what each may do, and who holds them are records in its
members space, written as the group with the `com.atproto.space.*`
writes [below](#signing-apps-in-as-a-group): `fyi.opensocial.role`, the
`fyi.opensocial.permissions` record, and `fyi.opensocial.membership`.
Writing a membership for someone waiting to join lets them in; writing
one for a member changes their roles; deleting one puts the member out
([membership](../design/membership.md#writing-roles-grants-and-memberships)).

| Refusal | Why |
| --- | --- |
| `LastConfigurer` | nobody would be left holding a role that configures the group |
| `RoleInUse` | a role deleted is still held, or still has a grant |
| `NotAssignable` | a role given, taken, or whose grant is changed, is not one the writer may assign |
| `ScopeNotHeld` | a scope given is not one the writer's roles hold |
| `MemberRole` | a membership leaves out `member` |
| `JoinRequestNotFound` | a membership made for someone not waiting to join |

A group invites someone by writing an `fyi.opensocial.invite` record in
their inbox as the group, and takes it back by deleting it, with the
same writes ([membership](../design/membership.md#invites)).

## People

| Method | What it does |
| --- | --- |
| `GET fyi.opensocial.getViewer` | who is asking, by the console's cookie or service auth, the other accounts the browser is signed in to (`accounts`), and where someone with no account makes one (`signUpAt`, `SIGNUP_PDS`) |
| `GET fyi.opensocial.getActor` | a person's DID and handle, to someone signed in: themselves, or anyone the host shows (a member of a group here, or someone asking to join one); to themselves also whether they may start a group |
| `GET fyi.opensocial.getActorGroups` | the groups here a person is in, as far as the caller shares them |
| `GET fyi.opensocial.searchActors` | accounts whose handle matches what is typed, from where `ACTOR_SEARCH` says; anyone may ask |
| `GET /img/<did>/<cid>` | a group's or a person's image, for whoever may see it: anyone, for one in a public repo; otherwise only someone signed in to the console |

## Spaces

A group's spaces, and the records in them ([spaces](../design/spaces.md)).

| Method | What it does |
| --- | --- |
| `POST fyi.opensocial.createSpace` | adds a space to a group |
| `POST fyi.opensocial.updateSpace` | changes a space's name and its access record |
| `POST fyi.opensocial.deleteSpace` | removes a space, never the meta or members space |
| `GET fyi.opensocial.listSpaceRecords` | the records in a space, from the host's index of it, for those signed in who may read it |
| `GET fyi.opensocial.getSpaceBlob` | a file in a space, read from whoever wrote it |

## Sites

A group's website ([sites](../design/sites.md)).

| Method | What it does |
| --- | --- |
| `POST fyi.opensocial.putSite` | publishes a group's site whole from the files given, each as text or bytes, in place of the one there, with the host's methods its pages may read (`reads`); takes a body as large as `BLOB_UPLOAD_LIMIT` |
| `POST fyi.opensocial.deleteSite` | takes a group's site down |
| `POST fyi.opensocial.putSiteDomain` | serves a group's site at its handle's own domain, once that domain answers as this host; its address and `www.` under the domain redirect there |
| `POST fyi.opensocial.deleteSiteDomain` | serves a group's site at its address alone again |
| `GET /tls/ask?domain=<domain>` | 200 if the proxy in front may get a certificate for `domain`: a group's own domain, or `www.` under it, while the group is on; else 404 |
| `GET`, `HEAD` `https://<address>/<path>` | a file of the site of the group whose address the request's `Host` names, sandboxed; the host keeps `/.well-known/` and `/xrpc/` there for itself. For a site that names `reads`, a page is the host's page around the site's, holding the visitor's session, with the site's page's title and preview tags and the status it would have |
| `GET https://<address>/_site/<path>` | for a site that names `reads`, the file the frame shows, sandboxed, as above |
| `GET https://<address>/client-metadata.json` | for a site that names `reads`, its OAuth client's metadata: a public client asking `atproto` and an `rpc:` scope for each read and `getViewer`, named for the request's `X-Forwarded-Host` if a proxy sends one, else its `Host` |
| `GET https://<address>/_os/kit.js` | for a site that names `reads`, the script its pages load for `os` |
| `GET https://<address>/_os/shell.js` | for a site that names `reads`, the host's page's script, from `SITE_SHELL_DIR`; 404 without it |

As the spaces' authority and the PDS of the group's own repo in each, the
host answers the space alpha's methods:

| Method | What it does |
| --- | --- |
| `POST com.atproto.space.authorizeSpaceSync` | a credential to read a space, for a user it lets (`delegationToken`) or anyone for a space open to all, bound to `credentialKey`, with the space's revision (`spaceRev`); with `notify`, registers an endpoint, once it confirms, to be told of changes until the credential lapses (`SpaceDeleted`, `RepoDeactivated`, `InvalidDelegationToken`, `RegistrationNotConfirmed`) |
| `POST com.atproto.space.notifyRepoUpdate` | told of a member's write in a space, by service auth from the writer (`FutureRev`) |
| `GET com.atproto.space.listWriters` | the space's writer set in the order of its revisions, each a `repoWrite` or a `repoRemoved`, a page of up to 1000 (100 unless asked); `cursor` is the last one's `spaceRev` |
| `POST com.atproto.space.notifyCredentialsRevoked` | told by a space's authority that credentials no longer hold |
| `GET com.atproto.space.getRecord`, `GET com.atproto.space.listRecords`, `GET com.atproto.space.getRepo`, `GET com.atproto.space.getLatestCommit`, `GET com.atproto.space.listRepoOps`, `GET com.atproto.space.getBlob`, `GET com.atproto.space.listBlobs` | the group's own repo in a space, for a credential that may read it. `getRecord`, `getRepo` (a STAR archive, `application/vnd.dasl.star`), `getLatestCommit` and `listRepoOps` (its `commit` on every page) need a MAC asked for in `Accept-Signature` (`InvalidSpaceMac`); a malformed cursor is `InvalidCursor` |

A service registered for a space is sent `com.atproto.space.notifySpaceUpdate` at its endpoint: each change to the writer set, a registration lapsed, and the space deleted ([spaces](../design/spaces.md#hearing-of-writes)).

## Signing apps in as a group

The host is each group's OAuth server ([signing in](../design/signing-in.md)):

| Route | What it does |
| --- | --- |
| `GET /.well-known/oauth-authorization-server`, `GET /.well-known/oauth-protected-resource` | its metadata |
| `POST /oauth/par`, `GET /oauth/authorize`, `POST /oauth/token`, `POST /oauth/revoke` | atproto OAuth's endpoints; `authorize` sends the browser to the console to approve |

| Method | What it does |
| --- | --- |
| `POST fyi.opensocial.provisionGroup` | an app `PROVISIONING_CLIENTS` names starts a group for a person, and is signed in to it |
| `POST fyi.opensocial.getGroupAuth` | a 15-minute token to act as the group in one of its spaces, for a member with a DPoP proof, scoped to what their roles may write there as the group, or in someone's inbox, to write invites there for a member who may invite; `Forbidden` if nothing |
| `GET fyi.opensocial.getAuthorization` | an app's request to sign in as a group, as the caller would approve it |
| `POST fyi.opensocial.approveAuthorization` | approves it, with what the caller's roles allow |
| `POST fyi.opensocial.declineAuthorization` | declines it |
| `GET fyi.opensocial.listGroupTokens` | the apps signed in as the group, and what they did lately, each with who let it in by their handle; a write also in its parts (`write`: operation, collection, key, space) and with whom it is about (`subject`) |
| `POST fyi.opensocial.revokeGroupToken` | signs one out |
| `POST fyi.opensocial.createAppPassword` | an app password for the group, for an app that signs in with one |
| `GET fyi.opensocial.listAppPasswords` | the group's app passwords, never the passwords, each with whose it is by their handle |
| `POST fyi.opensocial.revokeAppPassword` | revokes one, and signs out its apps |

An app signed in so, or with an app password, uses these as the group:

| Method | What it does |
| --- | --- |
| `GET com.atproto.server.getSession` | the group a token acts as: its DID, handle, and whether it is switched on |
| `POST com.atproto.server.createSession`, `POST com.atproto.server.refreshSession`, `POST com.atproto.server.deleteSession` | an app password's sessions |
| `POST com.atproto.repo.createRecord`, `POST com.atproto.repo.putRecord`, `POST com.atproto.repo.deleteRecord`, `POST com.atproto.repo.applyWrites`, `POST com.atproto.repo.uploadBlob` | writes to the group's public repo |
| `POST com.atproto.space.createRecord`, `POST com.atproto.space.putRecord`, `POST com.atproto.space.deleteRecord`, `POST com.atproto.space.applyWrites` | writes to its repo in a space: one of its own, or someone's inbox, where it writes and deletes its invites to them (`AlreadyMember`, `InviteNotFound`). A label (`com.atproto.label.label`) is written, and deleted, in any of the group's spaces, beside its subject; a member's record in their public repo is labelled in the members space ([moderation](../design/moderation.md#labels)) |

A group's profile and rules, in `meta`, change only this way: an app
writes the records as the group, and the host keeps each only as its
rules have it ([groups](../design/groups.md#changing-its-profile-rules-and-pictures)).

## MCP

An MCP client, an AI app, connects to a group over MCP ([MCP](../design/mcp.md));
its tools are in [MCP tools](mcp.md).

| Route | What it does |
| --- | --- |
| `POST /mcp` | the MCP server, each JSON-RPC message answered as the member who connected it |
| `GET /.well-known/oauth-protected-resource/mcp`, `GET /.well-known/oauth-authorization-server/mcp` | its metadata |
| `POST /mcp/register`, `GET /mcp/authorize`, `POST /mcp/token` | its own OAuth server |

| Method | What it does |
| --- | --- |
| `GET fyi.opensocial.listConnectedApps` | the MCP clients the caller connected to a group |
| `POST fyi.opensocial.revokeConnectedApp` | disconnects one |

## Moderation

| Method | What it does |
| --- | --- |
| `POST fyi.opensocial.createReport` | reports a member, a record in one of the group's spaces here, or a member's record in their public repo, from one who can read its space (the members space for a member or their public record) and as the group's labeler takes reports; answers the report's `id` and `createdAt`. A reason or note is 2,000 characters (20,000 bytes) at most |
| `GET fyi.opensocial.listSubjects` | the moderation queue of one status, `open` unless asked |
| `GET fyi.opensocial.getSubjectHistory` | a subject's events, oldest first, a page at a time; with a `cid`, those about that version |
| `POST fyi.opensocial.resolveSubject` | sets a subject `resolved` or `escalated`, with an optional note; answers the subject |
| `POST fyi.opensocial.putLabelDefinition` | defines one of the group's own labels, lowercase letters and hyphens, or changes it, in its labeler, for one with `group.configure`; answers the definition. `getGroup` answers them as `labels` |
| `POST fyi.opensocial.deleteLabelDefinition` | stops defining one; labels already applied stay |

## Operating the host

For its operators, the accounts `HOST_ADMINS` names ([config](config.md)),
by service auth or the console's cookie; anyone else is refused
`Forbidden`.

| Method | What it does |
| --- | --- |
| `GET fyi.opensocial.getLogs` | the lines the host logged, oldest first, each as it wrote it: one request's by its `trace`, or the last of a `message`, a `level` and above, or those that took `minMs` or longer; 100 unless `limit` asks for up to 500, or for a trace, 500. The read is itself logged, `logs read`, with who asked and for what |
| `GET fyi.opensocial.getStats` | how the host was used over the last `days` (7 unless asked, 400 at most), day by day in UTC, and what it holds: requests by route and by the class of their status, slow ones, MCP tools called by name, and groups and spaces made. Nothing in it names who asked |

Every request is logged as a `request` line, with its method, route,
path (never its query), status and `duration_ms`, and, when it is
refused or fails, its `error` and `error_message` as it answered them.
It is answered with `x-trace-id`, the trace each line logged
while answering it carries: one of its own, or the one a W3C
`traceparent` header names. The caller names that, so a trace read may
hold the lines of another request that named it too. A method other
than HTTP's own seven is counted as `OTHER`.

## Recovery

| Method | What it does |
| --- | --- |
| `GET fyi.opensocial.listRecoveryKeys` | the rotation keys on the group's `did:plc` that matter ([recovery](../design/recovery.md)) |
| `POST fyi.opensocial.addRecoveryKey` | adds a steward's key, ahead of the host's |
| `POST fyi.opensocial.removeRecoveryKey` | takes one off |

## Moving

Groups between hosts, and accounts here to become groups ([moving](../design/moving.md)).

| Method | What it does |
| --- | --- |
| `POST fyi.opensocial.migratePrepare` | on the host a group leaves: starts the move |
| `GET fyi.opensocial.migrateExport` | on that host: what the destination pulls of the group |
| `POST fyi.opensocial.migrateReceive` | on the destination: takes the group in, switched off |
| `POST fyi.opensocial.migrateComplete` | on the host it leaves: points its DID at the destination |
| `POST fyi.opensocial.migrateActivate` | on the destination: switches it on |
| `GET fyi.opensocial.checkImport` | what moving an account here would make of it |
| `POST fyi.opensocial.importAccount` | starts moving an account here |
| `POST fyi.opensocial.importAccountSpace` | brings one of its spaces, while the move waits |
| `POST fyi.opensocial.completeImport` | finishes the move |
| `GET fyi.opensocial.getAccountMove` | the account the caller signed in to move from the console |
| `POST fyi.opensocial.startAccountMove` | starts that move |
| `POST fyi.opensocial.finishAccountMove` | finishes it, with the code its server emailed |
| `POST fyi.opensocial.cancelAccountMove` | stops it |

## The console

| Route | What it does |
| --- | --- |
| `GET /console/login`, `GET /console/callback`, `POST /console/logout` | signing in to the console as oneself (`?handle=`), or signing up at `SIGNUP_PDS` (`?create`), and out; signing in again keeps the account before beside it, and signing out goes on as the one used last |
| `POST /console/switch` | switching to another account the browser is signed in to (`did`), and back to the page `back` |
| `GET /console/move/login` | signing in to an account to move here |
| `GET /console/client-metadata.json` | the host's metadata as the console's OAuth client |

## The PDS

The host is the PDS of every group it hosts, and answers for their public
repos as any PDS does:

| Method | What it does |
| --- | --- |
| `GET com.atproto.server.describeServer` | the host as a PDS: its DID, the domain it gives its groups names under, `BLOB_UPLOAD_LIMIT`, and that no one signs up |
| `GET com.atproto.repo.describeRepo`, `GET com.atproto.repo.getRecord`, `GET com.atproto.repo.listRecords` | a group's public repo |
| `GET com.atproto.sync.getRepo`, `GET com.atproto.sync.getLatestCommit`, `GET com.atproto.sync.listRepos`, `GET com.atproto.sync.getBlob`, `GET com.atproto.sync.listBlobs` | syncing them; a blob only a space holds is not served or listed. `listRepos` and `listBlobs` answer pages of up to 1000, 500 unless asked, as atproto's lexicons have them |
| `GET com.atproto.sync.subscribeRepos` | the firehose, over a WebSocket |
| `GET com.atproto.identity.resolveHandle` | a handle here, to its DID; for a caller signed in, any handle, through `HANDLE_RESOLVERS` |
| `GET /.well-known/did.json` | the host's own DID document, when `SERVICE_DID` is a `did:web`: its `#atproto_pds` and `#opensocial` services at `PUBLIC_URL` |
| `GET /.well-known/atproto-did` | the DID of the group whose address the request's host names |
| `GET /xrpc/_health` | that the host is up: its `version`, and the `commit` its image was built from (`null` in a build of a checkout) |

A switched-off group's public repo answers `RepoDeactivated`, and
`listRepos` shows it `active: false`.

## From a browser

The OAuth routes, the `/mcp` routes, and the methods an app calls with a
token (the writes, the sessions, `provisionGroup`, `getGroupAuth`) answer
any origin. The rest are for the console, at its own origin.

# HTTP and XRPC API

This is what the host answers over HTTP, at `PUBLIC_URL` ([config](config.md)).
Each `fyi.opensocial.*` method has its lexicon under
`lexicons/fyi/opensocial`, which says what it takes and answers, and the
site shows it under the method. The design pages say why each part works
the way it does.

An XRPC error is JSON, the same as atproto's servers send: an HTTP status
and `{"error", "message"}`. Besides the names a method's lexicon lists,
any method may answer these:

| Error | When |
| --- | --- |
| `InvalidRequest` | a parameter is missing or bad. The host checks a `fyi.opensocial.*` method's parameters and JSON input against its lexicon before anything else, and says where and what is wrong (`input.roles[2].role must not be longer than 64 bytes`) |
| `PayloadTooLarge` | a JSON input is over 2 MiB, or over `BLOB_UPLOAD_LIMIT` for `putSite` |
| `AuthMissing` | the method needs a caller and there isn't one |
| `BadJwt`, `BadJwtAudience`, `BadJwtIss`, `BadJwtLexiconMethod` | service auth doesn't hold up: a bad token, a token for another service, a signer whose key can't be found, or a token for another method |
| `Forbidden` | the caller's roles lack the action (`missing <action>`) |
| `GroupNotFound` | the DID isn't a group here |
| `RepoDeactivated` | the group is switched off |
| `InternalServerError` | the host's database, or a server it asked, failed |

The atproto methods answer with atproto's own names (`RepoNotFound`,
`BlobNotFound`, `InvalidSwap`, `CredentialRevoked`, …). When an app
writes a record as a group in one of our own `fyi.opensocial.*`
collections, the host checks it against its lexicon and answers
`validationStatus: "valid"`. It keeps a record of any other collection
without reading it and answers `unknown`. A GET or POST under `/xrpc/`
that no route answers gets 501 `MethodNotImplemented`.

## Who asks

The host's own methods know their caller in one of two ways:

- Service auth. That's `Authorization: Bearer <jwt>`, from the caller's
  own PDS (`com.atproto.server.getServiceAuth`), with the host's
  `SERVICE_DID` as `aud` (or `SERVICE_DID#<service>`, the way a PDS
  proxying to the host's `#opensocial` service names it) and the
  method's NSID as `lxm`. An app, or the `opensocial` command, calls as
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

| Method | What it does |
| --- | --- |
| `POST fyi.opensocial.createGroup` | starts a group, with the caller as its founder and first admin |
| `GET fyi.opensocial.getGroup` | shows a group as the caller may see it. Someone signed in who may read `meta` gets its profile and rules, and one who may read `members` also gets its member count, roles and spaces. Someone not signed in gets only its DID and handle |
| `GET fyi.opensocial.checkAddress` | says whether an address (one label under `HANDLE_DOMAIN`) is free for a new group. Anyone may ask |
| `GET fyi.opensocial.checkGroupHandle` | says whether a domain names the group yet, so it can be its handle |
| `POST fyi.opensocial.updateGroupHandle` | gives the group a handle on its own domain, or gives its address back |
| `POST fyi.opensocial.deactivateGroup` | switches a group off until `activateGroup` |
| `POST fyi.opensocial.activateGroup` | switches a deactivated group back on |
| `POST fyi.opensocial.deleteGroup` | deletes a group for good ([groups](../design/groups.md)) |

## Membership

| Method | What it does |
| --- | --- |
| `GET fyi.opensocial.listMembers` | lists the group's members and their roles, a page at a time in DID order (`limit`, `cursor`), filtered to those whose handle or DID contains `q`. It's for a caller who may read the members space |
| `POST fyi.opensocial.leaveGroup` | leaves a group, unless no one else could configure it (`LastConfigurer`) |
| `POST fyi.opensocial.requestJoin` | asks to join, or redeems an invite |
| `GET fyi.opensocial.getJoinRequest` | shows the caller's own request to join while it waits |
| `POST fyi.opensocial.cancelJoinRequest` | withdraws that request |
| `GET fyi.opensocial.listJoinRequests` | lists the waiting requests, for those who admit people |
| `POST fyi.opensocial.denyJoinRequest` | turns a request down |
| `GET fyi.opensocial.listInvites` | lists the group's invites that are still waiting, each with the invitee's handle |
| `GET fyi.opensocial.getActorInvites` | lists the caller's own invites from groups here |
| `POST fyi.opensocial.declineInvite` | turns an invite down |
| `GET com.atproto.managedspace.checkAccess` | tells an inbox's authority whether an account (`repo`) may read or write an inbox the host keeps, as `{read, write}` |

A group's roles, what each may do, and who holds them are records in its
members space: `fyi.opensocial.role`, the `fyi.opensocial.permissions`
record, and `fyi.opensocial.membership`. They're written as the group
with the `com.atproto.space.*` writes [below](#signing-apps-in-as-a-group).
Writing a membership for someone waiting to join lets them in. Writing
one for a member changes their roles, and deleting one puts the member
out ([membership](../design/membership.md#writing-roles-grants-and-memberships)).

| Refusal | Why |
| --- | --- |
| `LastConfigurer` | nobody would be left holding a role that configures the group |
| `RoleInUse` | a deleted role is still held, or still has a grant |
| `NotAssignable` | the writer may not assign a role being given or taken, or one whose grant is changing |
| `ScopeNotHeld` | the writer's roles don't hold a scope being given |
| `MemberRole` | a membership leaves out `member` |
| `JoinRequestNotFound` | a membership is made for someone who isn't waiting to join |

A group invites someone by writing an `fyi.opensocial.invite` record in
their inbox as the group. It takes the invite back by deleting it, with
the same writes ([membership](../design/membership.md#invites)).

## People

| Method | What it does |
| --- | --- |
| `GET fyi.opensocial.getViewer` | says who's asking, by the console's cookie or service auth. It also lists the other accounts the browser is signed in to (`accounts`), and where someone without an account makes one (`signUpAt`, `SIGNUP_PDS`) |
| `GET fyi.opensocial.getActor` | gives a person's DID and handle to someone signed in, about themselves or anyone the host shows (a member of a group here, or someone asking to join one). Asked about themselves, it also says whether they may start a group |
| `GET fyi.opensocial.getActorGroups` | lists the groups here a person is in, as far as the caller shares them |
| `GET fyi.opensocial.searchActors` | finds accounts whose handle matches what's typed, from the source `ACTOR_SEARCH` names. Anyone may ask |
| `GET /img/<did>/<cid>` | serves a group's or a person's image to anyone who may see it. That's anyone for an image in a public repo, and otherwise only someone signed in to the console |

## Spaces

A group's spaces, and the records in them ([spaces](../design/spaces.md)).

| Method | What it does |
| --- | --- |
| `POST fyi.opensocial.createSpace` | adds a space to a group |
| `POST fyi.opensocial.updateSpace` | changes a space's name and its access record |
| `POST fyi.opensocial.deleteSpace` | removes a space. It never removes the meta or members space |
| `GET fyi.opensocial.listSpaceRecords` | lists the records in a space from the host's index of it, for someone signed in who may read it |
| `GET fyi.opensocial.getSpaceBlob` | serves a file in a space, read from the repo of whoever wrote it |

## Sites

A group's website ([sites](../design/sites.md)).

| Method | What it does |
| --- | --- |
| `POST fyi.opensocial.putSite` | publishes a group's whole site from the files given, each as text or bytes, and replaces the one there. `reads` lists the host's methods its pages may read, `acts` the procedures they may ask, and `spaces` the group's space types they may read whole. `writes` lists the records they may write in the signed-in person's own repo in the group's spaces. Pages do all of it as that person. It takes a body up to `BLOB_UPLOAD_LIMIT` |
| `POST fyi.opensocial.deleteSite` | takes a group's site down |
| `POST fyi.opensocial.putSiteDomain` | serves a group's site at its handle's own domain once that domain answers as this host. The site's address and `www.` under the domain redirect there |
| `POST fyi.opensocial.deleteSiteDomain` | serves a group's site at its address alone again |
| `GET /tls/ask?domain=<domain>` | answers 200 if the proxy in front may get a certificate for `domain`, and 404 otherwise. A domain qualifies if it's a group's own domain, or `www.` under it, while the group is on |
| `GET`, `HEAD` `https://<address>/<path>` | serves a file, sandboxed, from the site of the group whose address the request's `Host` names. The host keeps `/.well-known/` and `/xrpc/` there for itself. For a site that names `reads`, `spaces`, `acts` or `writes`, a page is the host's page wrapped around the site's. It holds the visitor's session and carries the site page's title, preview tags and the status it would have |
| `GET https://<address>/_site/<path>` | for a site that names `reads`, `spaces`, `acts` or `writes`, serves the file the frame shows, sandboxed, as above |
| `GET https://<address>/client-metadata.json` | for a site that names `reads`, `spaces`, `acts` or `writes`, serves its OAuth client's metadata. It's a public client asking for `atproto`, an `rpc:` scope for each read, each act and `getViewer`, and a `space:` scope to read each space type, with `authority` the group. For each write it asks a `space:` scope of its type, with `authority` the group, its collection and every action, and with any write it asks `blob:image/*`. It's named for the request's `X-Forwarded-Host` if a proxy sends one, and otherwise its `Host` |
| `GET https://<address>/_os/kit.js` | for a site that names `reads`, `spaces`, `acts` or `writes`, serves the script its pages load for `os` ([the site kit](site-kit.md)) |
| `GET https://<address>/_os/shell.js` | for a site that names `reads`, `spaces`, `acts` or `writes`, serves the host's page script from `SITE_SHELL_DIR`, or 404 without it |

The host is the spaces' authority and the PDS of the group's own repo in
each, so it answers the space alpha's methods:

| Method | What it does |
| --- | --- |
| `POST com.atproto.space.authorizeSpaceSync` | gives a credential to read a space, bound to `credentialKey`, with the space's revision (`spaceRev`). It gives one to a user it lets in (`delegationToken`), or to anyone for a space open to all. With `notify`, it registers an endpoint, once the endpoint confirms, to hear of changes until the credential lapses (`SpaceDeleted`, `RepoDeactivated`, `InvalidDelegationToken`, `RegistrationNotConfirmed`) |
| `POST com.atproto.space.notifyRepoUpdate` | hears about a member's write in a space, by service auth from the writer (`FutureRev`) |
| `GET com.atproto.space.listWriters` | lists the space's writer set in the order of its revisions, each a `repoWrite` or a `repoRemoved`. A page holds up to 1000 (100 by default), and `cursor` is the last one's `spaceRev` |
| `POST com.atproto.space.notifyCredentialsRevoked` | hears from a space's authority that credentials no longer hold |
| `GET com.atproto.space.getRecord`, `GET com.atproto.space.listRecords`, `GET com.atproto.space.getRepo`, `GET com.atproto.space.getLatestCommit`, `GET com.atproto.space.listRepoOps`, `GET com.atproto.space.getBlob`, `GET com.atproto.space.listBlobs` | serve the group's own repo in a space, for a credential that may read it. `getRecord`, `getRepo` (a STAR archive, `application/vnd.dasl.star`), `getLatestCommit` and `listRepoOps` (its `commit` on every page) need a MAC asked for in `Accept-Signature` (`InvalidSpaceMac`). A malformed cursor fails with `InvalidCursor` |

The host sends `com.atproto.space.notifySpaceUpdate` to the endpoint of
a service registered for a space. It sends one for each change to the
writer set, for a lapsed registration, and when the space is deleted
([spaces](../design/spaces.md#hearing-of-writes)).

## Signing apps in as a group

The host is each group's OAuth server ([signing in](../design/signing-in.md)):

| Route | What it does |
| --- | --- |
| `GET /.well-known/oauth-authorization-server`, `GET /.well-known/oauth-protected-resource` | its metadata |
| `POST /oauth/par`, `GET /oauth/authorize`, `POST /oauth/token`, `POST /oauth/revoke` | atproto OAuth's endpoints. `authorize` sends the browser to the console to approve |

| Method | What it does |
| --- | --- |
| `POST fyi.opensocial.provisionGroup` | lets an app that `PROVISIONING_CLIENTS` names start a group for a person, and signs the app in to it |
| `POST fyi.opensocial.getGroupAuth` | gives a member with a DPoP proof a 15-minute token to act as the group. In one of the group's spaces, it's scoped to what their roles may write there as the group. In someone's inbox, it lets a member who may invite write invites there. It answers `Forbidden` if that's nothing |
| `GET fyi.opensocial.getAuthorization` | shows an app's request to sign in as a group, as the caller would approve it |
| `POST fyi.opensocial.approveAuthorization` | approves the request with what the caller's roles allow |
| `POST fyi.opensocial.declineAuthorization` | declines the request |
| `GET fyi.opensocial.listGroupTokens` | lists the apps signed in as the group and what they did lately, each with the handle of whoever let it in. A write also comes in its parts (`write`: operation, collection, key, space) and with whom it's about (`subject`) |
| `POST fyi.opensocial.revokeGroupToken` | signs one app out |
| `POST fyi.opensocial.createAppPassword` | makes an app password for the group, for an app that signs in with one |
| `GET fyi.opensocial.listAppPasswords` | lists the group's app passwords, each with its owner's handle. It never shows the passwords |
| `POST fyi.opensocial.revokeAppPassword` | revokes one, and signs out its apps |

An app signed in this way, or with an app password, uses these as the
group:

| Method | What it does |
| --- | --- |
| `GET com.atproto.server.getSession` | describes the group a token acts as: its DID, its handle, and whether it's switched on |
| `POST com.atproto.server.createSession`, `POST com.atproto.server.refreshSession`, `POST com.atproto.server.deleteSession` | handle an app password's sessions |
| `POST com.atproto.repo.createRecord`, `POST com.atproto.repo.putRecord`, `POST com.atproto.repo.deleteRecord`, `POST com.atproto.repo.applyWrites`, `POST com.atproto.repo.uploadBlob` | write to the group's public repo |
| `POST com.atproto.space.createRecord`, `POST com.atproto.space.putRecord`, `POST com.atproto.space.deleteRecord`, `POST com.atproto.space.applyWrites` | write to the group's repo in a space. That's one of its own spaces, or someone's inbox, where it writes and deletes its invites to them (`AlreadyMember`, `InviteNotFound`). A label (`com.atproto.label.record`) is written and deleted in any of the group's spaces, beside its subject. A label on a member's record in their public repo goes in the members space ([moderation](../design/moderation.md#labels)) |

A group's profile and rules, in `meta`, change only this way. An app
writes the records as the group, and the host keeps each one only as the
rules allow ([groups](../design/groups.md#changing-its-profile-rules-and-pictures)).

## MCP

An MCP client (an AI app) connects to a group over MCP ([MCP](../design/mcp.md)).
Its tools are in [MCP tools](mcp.md).

| Route | What it does |
| --- | --- |
| `POST /mcp` | the MCP server, which answers each JSON-RPC message as the member who connected the client |
| `GET /.well-known/oauth-protected-resource/mcp`, `GET /.well-known/oauth-authorization-server/mcp` | its metadata |
| `POST /mcp/register`, `GET /mcp/authorize`, `POST /mcp/token` | its own OAuth server |

| Method | What it does |
| --- | --- |
| `GET fyi.opensocial.listConnectedApps` | lists the MCP clients the caller connected to a group |
| `POST fyi.opensocial.revokeConnectedApp` | disconnects one |

## Moderation

| Method | What it does |
| --- | --- |
| `POST fyi.opensocial.createReport` | reports a member, a record in one of the group's spaces here, or a member's record in their public repo. The reporter must be able to read its space (the members space for a member or their public record), and the group's labeler must take reports. It answers the report's `id` and `createdAt`. With `reasonType` `com.atproto.moderation.defs#reasonAppeal` it's an appeal, and only its subject may file one (the account itself, or the record's author). Anyone else gets `InvalidRequest`. The host takes an appeal even from someone put out, as long as the queue already has the subject. A reason or note is at most 2,000 characters (20,000 bytes) |
| `GET fyi.opensocial.listSubjects` | lists the moderation queue for one status, `open` by default |
| `GET fyi.opensocial.getSubjectHistory` | lists a subject's events, oldest first, a page at a time. With a `cid`, it lists those about that version |
| `POST fyi.opensocial.resolveSubject` | sets a subject `resolved` or `escalated`, with an optional note, and answers the subject |
| `POST fyi.opensocial.putLabelDefinition` | defines or changes one of the group's own labels (lowercase letters and hyphens) in its labeler, for someone with `group.configure`. It answers the definition, and `getGroup` answers them as `labels` |
| `POST fyi.opensocial.deleteLabelDefinition` | stops defining one. Labels already applied stay |

## Operating the host

These are for its operators, the accounts `HOST_ADMINS` names
([config](config.md)), by service auth or the console's cookie. The host
refuses anyone else with `Forbidden`.

| Method | What it does |
| --- | --- |
| `GET fyi.opensocial.getLogs` | returns the lines the host logged, oldest first, each as it wrote it. It returns one request's lines by its `trace`, or the last lines of a `message`, at a `level` and above, or that took `minMs` or longer. It returns 100 unless `limit` asks for up to 500, and 500 for a trace. The read itself is logged as `logs read`, with who asked and for what |
| `GET fyi.opensocial.getStats` | shows how the host was used over the last `days` (7 by default, 400 at most), day by day in UTC, and what it holds. It counts requests by route and by status class, slow ones, MCP tools called by name, and groups and spaces made. Nothing in it names who asked |

The host logs every request as a `request` line, with its method, route,
path (never its query), status and `duration_ms`. When it refuses or
fails a request, the line also has the `error` and `error_message` it
answered. Each response carries `x-trace-id`, the trace that every line
logged while answering it carries. The trace is the host's own, or the
one a W3C `traceparent` header names. Since the caller names that one, a
trace read may hold the lines of another request that named it too. A
method other than HTTP's own seven is counted as `OTHER`.

## Recovery

| Method | What it does |
| --- | --- |
| `GET fyi.opensocial.listRecoveryKeys` | lists the rotation keys on the group's `did:plc` that matter ([recovery](../design/recovery.md)) |
| `POST fyi.opensocial.addRecoveryKey` | adds a steward's key, ahead of the host's |
| `POST fyi.opensocial.removeRecoveryKey` | takes one off |

## Moving

Moving groups between hosts, and accounts here to become groups ([moving](../design/moving.md)).

| Method | What it does |
| --- | --- |
| `POST fyi.opensocial.migratePrepare` | on the host a group leaves, starts the move |
| `GET fyi.opensocial.migrateExport` | on that host, serves what the destination pulls of the group |
| `POST fyi.opensocial.migrateReceive` | on the destination, takes the group in, switched off |
| `POST fyi.opensocial.migrateComplete` | on the host it leaves, points its DID at the destination |
| `POST fyi.opensocial.migrateActivate` | on the destination, switches it on |
| `GET fyi.opensocial.checkImport` | shows what moving an account here would make of it |
| `POST fyi.opensocial.importAccount` | starts moving an account here |
| `POST fyi.opensocial.importAccountSpace` | brings over one of its spaces while the move waits |
| `POST fyi.opensocial.completeImport` | finishes the move |
| `GET fyi.opensocial.getAccountMove` | shows the account the caller signed in to from the console to move |
| `POST fyi.opensocial.startAccountMove` | starts that move |
| `POST fyi.opensocial.finishAccountMove` | finishes it with the code the account's server emailed |
| `POST fyi.opensocial.cancelAccountMove` | stops it |

## The console

| Route | What it does |
| --- | --- |
| `GET /console/login`, `GET /console/callback`, `POST /console/logout` | sign in to the console as yourself (`?handle=`), sign up at `SIGNUP_PDS` (`?create`), and sign out. Signing in again keeps the earlier account beside the new one, and signing out switches to the one used last |
| `POST /console/switch` | switches to another account the browser is signed in to (`did`), and goes back to the page `back` |
| `GET /console/move/login` | signs in to an account to move here |
| `GET /console/client-metadata.json` | the host's metadata as the console's OAuth client |

## The PDS

The host is the PDS of every group it hosts, and answers for their public
repos as any PDS does:

| Method | What it does |
| --- | --- |
| `GET com.atproto.server.describeServer` | describes the host as a PDS: its DID, the domain it names its groups under, `BLOB_UPLOAD_LIMIT`, and that no one signs up |
| `GET com.atproto.repo.describeRepo`, `GET com.atproto.repo.getRecord`, `GET com.atproto.repo.listRecords` | serve a group's public repo |
| `GET com.atproto.sync.getRepo`, `GET com.atproto.sync.getLatestCommit`, `GET com.atproto.sync.listRepos`, `GET com.atproto.sync.getBlob`, `GET com.atproto.sync.listBlobs` | sync the repos. A blob only a space holds isn't served or listed. `listRepos` and `listBlobs` answer pages of up to 1000 (500 by default), as atproto's lexicons have them |
| `GET com.atproto.sync.subscribeRepos` | the firehose, over a WebSocket |
| `GET com.atproto.identity.resolveHandle` | resolves a handle here to its DID. For a caller who's signed in, it resolves any handle through `HANDLE_RESOLVERS` |
| `GET /.well-known/did.json` | the host's own DID document when `SERVICE_DID` is a `did:web`, with its `#atproto_pds` and `#opensocial` services at `PUBLIC_URL` |
| `GET /.well-known/atproto-did` | the DID of the group whose address the request's host names |
| `GET /xrpc/_health` | says the host is up, with its `version` and the `commit` its image was built from (`null` in a build of a checkout) |

A switched-off group's public repo answers `RepoDeactivated`, and
`listRepos` shows it as `active: false`.

## From a browser

The OAuth routes, the `/mcp` routes, and the methods an app calls with a
token (the writes, the sessions, `provisionGroup`, `getGroupAuth`) answer
any origin. So do the methods a space is read by. Those are its
authority's `authorizeSpaceSync` and `listWriters`, and the reads of a
repo in it (`getRepo`, `listRepoOps` and the rest), which
`@opensocial/sdk` makes in a site's page. The rest are for the console,
at its own origin.
